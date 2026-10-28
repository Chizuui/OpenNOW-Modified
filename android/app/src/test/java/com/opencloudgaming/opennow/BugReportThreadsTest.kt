package com.opencloudgaming.opennow

import okio.Buffer
import okhttp3.MultipartBody
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class BugReportThreadsTest {
    @Test
    fun parsesPrivateReporterInboxWithStatusAndConversation() {
        val reports = parseAndroidBugReportThreads(
            """
            {
              "ok": true,
              "data": {
                "reports": [{
                  "id": "report-123",
                  "title": "Stream froze",
                  "description": "Video stopped after reconnecting.",
                  "status": "planned_next_update",
                  "resolutionNote": "A fix is queued for the next update.",
                  "versionName": "1.7.9",
                  "versionCode": "135",
                  "kind": "performance",
                  "area": "streaming",
                  "frequency": "sometimes",
                  "impact": "high",
                  "files": [{"name":"opennow.log"},{"name":"freeze.mp4"}],
                  "createdAt": 100,
                  "updatedAt": 300,
                  "comments": [
                    {"id":"one","authorRole":"reporter","kind":"comment","body":"I can reproduce this.","createdAt":200},
                    {"id":"two","authorRole":"admin","kind":"status","body":"Status changed.","createdAt":300}
                  ]
                }]
              }
            }
            """.trimIndent(),
        )

        assertEquals(1, reports.size)
        assertEquals("report-123", reports.single().id)
        assertEquals("planned_next_update", reports.single().status)
        assertEquals("A fix is queued for the next update.", reports.single().resolutionNote)
        assertEquals("performance", reports.single().kind)
        assertEquals(listOf("opennow.log", "freeze.mp4"), reports.single().files)
        assertEquals(listOf("reporter", "admin"), reports.single().comments.map { it.authorRole })
        assertEquals(listOf("comment", "status"), reports.single().comments.map { it.kind })
    }

    @Test
    fun completedAndDeclinedReportsAreClosed() {
        assertTrue(androidBugReportThreadClosed("completed"))
        assertTrue(androidBugReportThreadClosed("not_reproducible"))
        assertTrue(androidBugReportThreadClosed("wont_fix"))
        assertTrue(androidBugReportThreadClosed("closed_by_reporter"))
        assertTrue(!androidBugReportThreadClosed("needs_info"))
    }

    @Test
    fun malformedInboxDoesNotExposePartialServerPayloads() {
        val error = runCatching { parseAndroidBugReportThreads("<html>proxy error</html>") }.exceptionOrNull()
        assertTrue(error is AndroidBugReportUploadException)
        assertEquals("INVALID_RESPONSE", (error as AndroidBugReportUploadException).serverCode)
        assertTrue(parseAndroidBugReportThreads("{\"data\":{\"reports\":[{}]}}").isEmpty())
    }
    @Test
    fun uploadsReplyFilesAndLogsAsMultipartWithReporterAndTerms() {
        val reporter = androidBugReportReporterId("thread-upload-test")
        val request = buildAndroidBugReportCommentRequest(
            reporterId = reporter,
            reportId = "report/with space",
            comment = "  New reproduction  ",
            files = listOf(
                AndroidBugReportAttachment("new.log", "text/plain", "sanitized log".toByteArray()),
                AndroidBugReportAttachment("capture.png", "image/png", byteArrayOf(1, 2, 3)),
            ),
            endpoint = "https://example.com/reports",
        )
        assertEquals("https://example.com/reports/report%2Fwith%20space/comments", request.url.toString())
        assertEquals(reporter, request.header("X-Bug-Report-Reporter-Id"))
        val body = request.body as MultipartBody
        assertEquals(6, body.parts.size)
        val output = Buffer()
        body.writeTo(output)
        val multipart = output.readUtf8()
        assertTrue(multipart.contains("name=\"comment\"\r\n\r\nNew reproduction"))
        assertTrue(multipart.contains("name=\"termsAccepted\"\r\n\r\ntrue"))
        assertTrue(multipart.contains("name=\"reporterId\"\r\n\r\n$reporter"))
        assertTrue(multipart.contains("name=\"files\"; filename=\"new.log\""))
        assertTrue(multipart.contains("sanitized log"))
        assertTrue(multipart.contains("name=\"files\"; filename=\"capture.png\""))
    }

    @Test
    fun acceptsAttachmentOnlyRepliesAndKeepsTextOnlyRepliesCompatible() {
        val reporter = androidBugReportReporterId("thread-upload-test")
        val file = AndroidBugReportAttachment("new.log", "text/plain", byteArrayOf(1))
        assertTrue(buildAndroidBugReportCommentRequest(reporter, "report", "", listOf(file)).body is MultipartBody)
        val textRequest = buildAndroidBugReportCommentRequest(reporter, "report", "More detail")
        assertEquals("application/json; charset=utf-8", textRequest.body?.contentType().toString())
        val output = Buffer()
        textRequest.body!!.writeTo(output)
        assertTrue(output.readUtf8().contains("\"comment\":\"More detail\""))
        assertTrue(runCatching { buildAndroidBugReportCommentRequest(reporter, "report", "  ") }.exceptionOrNull() is IllegalArgumentException)
    }

    @Test
    fun rejectsTooManyOrOversizedReplyAttachmentsBeforeSending() {
        val reporter = androidBugReportReporterId("thread-upload-test")
        val file = AndroidBugReportAttachment("new.log", "text/plain", byteArrayOf(1))
        assertTrue(runCatching {
            buildAndroidBugReportCommentRequest(reporter, "report", "Files", List(6) { file })
        }.exceptionOrNull() is IllegalArgumentException)
        assertTrue(runCatching {
            buildAndroidBugReportCommentRequest(reporter, "report", "Files", listOf(file.copy(bytes = ByteArray(10 * 1024 * 1024 + 1))))
        }.exceptionOrNull() is IllegalArgumentException)
    }

    @Test
    fun parsesReplyAttachmentsUnderTheirMessageWithoutDuplicatingOriginalFiles() {
        val report = parseAndroidBugReportThreadResponse(
            """{"data":{"id":"report","files":[{"name":"original.log"},{"name":"new.log","commentId":"reply"}],
                "comments":[{"id":"reply","authorRole":"reporter","body":"","files":[{"name":"new.log"}]}]}}""",
        )!!
        assertEquals(listOf("original.log"), report.files)
        assertEquals(listOf("new.log"), report.comments.single().files)
    }

}
