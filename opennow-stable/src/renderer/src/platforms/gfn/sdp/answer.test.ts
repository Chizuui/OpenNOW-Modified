/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import { ensureAudioRedInAnswer, mungeAnswerSdp } from "./answer";

test("mungeAnswerSdp injects bitrate lines and appends opus stereo once", () => {
  const sdp = [
    "m=video 9 UDP/TLS/RTP/SAVPF 98",
    "c=IN IP4 127.0.0.1",
    "m=audio 9 UDP/TLS/RTP/SAVPF 111",
    "a=fmtp:111 minptime=10;useinbandfec=1",
  ].join("\n");

  const munged = mungeAnswerSdp(sdp, 50000);
  assert.match(munged, /m=video.*\nb=AS:50000\n/);
  assert.match(munged, /m=audio.*\nb=AS:128\n/);
  assert.match(munged, /a=fmtp:111 minptime=10;useinbandfec=1;stereo=1/);

  const alreadyStereo = mungeAnswerSdp("m=audio 9 UDP/TLS/RTP/SAVPF 111\nb=AS:128\na=fmtp:111 minptime=10;stereo=1", 50000);
  assert.equal((alreadyStereo.match(/stereo=1/g) ?? []).length, 1);
  assert.equal((alreadyStereo.match(/b=AS:128/g) ?? []).length, 1);
});

test("mungeAnswerSdp preserves CRLF and treats any existing bandwidth line as authoritative", () => {
  const sdp = [
    "m=video 9 UDP/TLS/RTP/SAVPF 98",
    "b=TIAS:50000000",
    "m=audio 9 UDP/TLS/RTP/SAVPF 111",
    "b=AS:96",
    "a=fmtp:111 minptime=10",
    "",
  ].join("\r\n");

  const munged = mungeAnswerSdp(sdp, 50000);

  assert.equal(munged, sdp.replace("minptime=10", "minptime=10;stereo=1"));
  assert.doesNotMatch(munged, /b=AS:50000/);
  assert.match(munged, /\r\n/);
});

test("ensureAudioRedInAnswer mirrors the offer RED payload into the matching audio section", () => {
  const offer = [
    "v=0",
    "m=audio 9 UDP/TLS/RTP/SAVPF 63 111",
    "a=mid:0",
    "a=rtpmap:63 red/48000/2",
    "a=fmtp:63 111/111",
    "a=rtpmap:111 opus/48000/2",
    "a=fmtp:111 minptime=10;useinbandfec=1",
    "m=audio 9 UDP/TLS/RTP/SAVPF 0",
    "a=mid:1",
    "a=rtpmap:0 PCMU/8000",
  ].join("\n");

  const answer = [
    "v=0",
    "m=audio 9 UDP/TLS/RTP/SAVPF 111",
    "a=mid:0",
    "a=rtpmap:111 opus/48000/2",
    "a=fmtp:111 minptime=10;useinbandfec=1;stereo=1",
    "m=audio 9 UDP/TLS/RTP/SAVPF 0",
    "a=mid:1",
    "a=rtpmap:0 PCMU/8000",
  ].join("\n");

  const munged = ensureAudioRedInAnswer(answer, offer, true);

  // RED payload re-advertised on the game-audio m-line only.
  assert.match(munged, /m=audio 9 UDP\/TLS\/RTP\/SAVPF 111 63/);
  assert.match(munged, /a=rtpmap:63 red\/48000\/2/);
  assert.match(munged, /a=fmtp:63 111\/111/);
  // The other audio section is untouched and there is no duplicate rtpmap.
  assert.match(munged, /m=audio 9 UDP\/TLS\/RTP\/SAVPF 0/);
  assert.equal(munged.match(/a=rtpmap:63 red\/48000\/2/g)?.length, 1);
});

test("ensureAudioRedInAnswer never negotiates RED when unsupported or already present", () => {
  const offer = [
    "m=audio 9 UDP/TLS/RTP/SAVPF 63 111",
    "a=mid:0",
    "a=rtpmap:63 red/48000/2",
    "a=fmtp:63 111/111",
    "a=rtpmap:111 opus/48000/2",
  ].join("\n");

  const answer = [
    "m=audio 9 UDP/TLS/RTP/SAVPF 111",
    "a=mid:0",
    "a=rtpmap:111 opus/48000/2",
  ].join("\n");

  // Unsupported receive path: the answer is returned untouched.
  assert.equal(ensureAudioRedInAnswer(answer, offer, false), answer);

  // Already negotiated: no double injection.
  const already = [
    "m=audio 9 UDP/TLS/RTP/SAVPF 111 63",
    "a=mid:0",
    "a=rtpmap:111 opus/48000/2",
    "a=rtpmap:63 red/48000/2",
    "a=fmtp:63 111/111",
  ].join("\n");
  assert.equal(ensureAudioRedInAnswer(already, offer, true), already);

  // Offer without RED: nothing to mirror.
  const plainOffer = [
    "m=audio 9 UDP/TLS/RTP/SAVPF 111",
    "a=mid:0",
    "a=rtpmap:111 opus/48000/2",
  ].join("\n");
  assert.equal(ensureAudioRedInAnswer(answer, plainOffer, true), answer);
});

test("ensureAudioRedInAnswer preserves the answer line ending style", () => {
  const offer = [
    "m=audio 9 UDP/TLS/RTP/SAVPF 63 111",
    "a=mid:0",
    "a=rtpmap:63 red/48000/2",
    "a=fmtp:63 111/111",
    "a=rtpmap:111 opus/48000/2",
  ].join("\r\n");

  const answer = [
    "m=audio 9 UDP/TLS/RTP/SAVPF 111",
    "a=mid:0",
    "a=rtpmap:111 opus/48000/2",
  ].join("\r\n");

  const munged = ensureAudioRedInAnswer(answer, offer, true);
  assert.match(munged, /m=audio 9 UDP\/TLS\/RTP\/SAVPF 111 63/);
  assert.match(munged, /a=rtpmap:63 red\/48000\/2\r\na=fmtp:63 111\/111/);
  // No bare-LF line slipped in.
  assert.equal(munged.replace(/\r\n/g, "").includes("\n"), false);
});
