import QtQuick
import OpenNOW

QtObject {
    property QtObject client: QtObject {
        property string state: "ready"
        property string lastError: ""
        property int sequence: 0
        property var requests: []
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        signal eventReceived(string name, var payload)
        function markUiReady() {}
        function logShellDiagnostic(message) {}
        function request(method, params, timeout) {
            const id = "catalog-test-" + (++sequence)
            requests.push({id:id, method:method, params:params})
            return id
        }
        function cancel(id) { requestFailed(id, "cancelled", "Cancelled"); return true }
    }
    function check(value, message) { if (!value) throw new Error("Catalog sync: " + message) }
    function namedChild(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = namedChild(child, name)
            if (found) return found
        }
        return null
    }
    function game(id) {
        return {id:"app-" + id, uuid:"app-" + id, title:"Library game " + id,
            variants:[{id:String(id + 100), store:"STEAM", inLibrary:true, libraryStatus:"MANUAL"}],
            imageUrl:"", availableStores:["STEAM"], genres:["ACTION"], isInLibrary:true}
    }
    function deliver(owner, games, cursor, more) {
        client.responseReceived(owner.catalogRequestId, {games:games,totalCount:1300,hasNextPage:more,
            nextCursor:cursor,traversalId:owner.catalogTraversalId,catalogRevision:0,catalogContext:"fixture-context"})
        owner.libraryPageTimer.stop()
    }
    function run(host) {
        ShellStore.authSession = {user:{userId:"catalog-fixture",displayName:"Catalog fixture"},provider:{idpId:"fixture"}}
        const owner = ShellStore.catalogOwnerState
        const accounts = ShellStore.accountServicesOwnerState
        check(accounts.gameAccountAction({provider:"STEAM",isConnected:false,supportsSync:true,supportsLinking:false}) === "sync", "first Steam sync was routed to unsupported linking")
        check(accounts.gameAccountAction({provider:"UPLAY",isConnected:true,supportsSync:true,supportsLinking:true,syncState:"SYNC_DENIED"}) === "link", "denied authorization did not offer reconnect")
        check(accounts.gameAccountAction({provider:"NEW_STORE",isConnected:false,supportsSync:false,supportsLinking:false}) === "none", "unknown store advertised an action")
        owner.reloadCatalogForSession()
        for (let page = 0; page < 13; ++page) {
            if (page) owner.requestLibraryPage()
            const request = client.requests.find(item => item.id === owner.catalogRequestId)
            check(request.method === "catalog.library.list" && request.params.limit === 100, "library page contract")
            const games = []
            for (let index = 0; index < 100; ++index) games.push(game(page * 100 + index))
            deliver(owner, games, "cursor-" + page, page < 12)
            check(owner.catalogComplete === (page === 12), "partial page claimed completion")
        }
        check(owner.catalogGames.length === 1300, "thousand-game ceiling remains")
        const completedAt = owner.catalogLastCompleteAt
        owner.refreshCatalog("")
        deliver(owner, [game(9000)], "refresh-two", true)
        check(owner.catalogGames.length === 1300, "refresh replaced the last complete snapshot early")
        owner.requestLibraryPage()
        client.requestFailed(owner.catalogRequestId, "upstream_error", "Fixture HTTP 503")
        check(owner.catalogGames.length === 1300 && owner.catalogError.indexOf("503") >= 0, "later failure lost usable data or error")
        owner.continueCatalog()
        check(client.requests.find(item => item.id === owner.catalogRequestId).params.cursor === "refresh-two", "retry skipped the failed page")
        deliver(owner, [game(9001)], "", false)
        check(owner.catalogGames.length === 2 && owner.catalogLastCompleteAt >= completedAt, "validated end did not commit removals")
        owner.refreshCatalog("")
        deliver(owner, [game(1)], "cycle-a", true)
        owner.requestLibraryPage()
        deliver(owner, [game(2)], "cycle-b", true)
        owner.requestLibraryPage()
        deliver(owner, [game(3)], "cycle-a", true)
        check(owner.catalogState === "partial" && owner.catalogNextCursor === "", "nonadjacent cursor cycle was accepted")
        owner.refreshCatalog("")
        owner.catalogSliceStarted = Date.now() - 31000
        deliver(owner, [game(1)], "slice-two", true)
        check(owner.catalogState === "partial" && owner.catalogNextCursor === "slice-two", "foreground slice was not resumable")
        owner.continueCatalog()
        deliver(owner, [game(2)], "", false)
        owner.refreshCatalog("")
        deliver(owner, [game(1)], "__proto__", true)
        check(owner.catalogNextCursor === "__proto__" && owner.catalogError === "", "opaque cursor collided with a JavaScript property")
        owner.requestLibraryPage()
        deliver(owner, [game(2)], "", false)
        accounts.syncGameAccount("STEAM")
        const before = client.requests.filter(item => item.method === "catalog.library.list").length
        client.responseReceived(accounts.gameAccountActionRequestId, {operationId:"sync-one",provider:"STEAM",phase:"waiting_remote"})
        check(client.requests.filter(item => item.method === "catalog.library.list").length === before, "202 started the final refresh")
        accounts.syncPollTimer.stop()
        accounts.pollSync()
        client.responseReceived(accounts.syncStatusRequestId, {operationId:"sync-one",provider:"STEAM",phase:"refreshing_library"})
        check(accounts.syncOperation.phase === "refreshing_library", "remote completion skipped refresh phase")
        deliver(owner, [game(1),game(2)], "", false)
        check(accounts.syncOperation === null && accounts.gameAccountMessage.indexOf("completed") >= 0, "complete traversal did not finish sync")
        accounts.syncGameAccount("STEAM")
        client.responseReceived(accounts.gameAccountActionRequestId, {operationId:"sync-two",provider:"STEAM",phase:"waiting_remote"})
        accounts.cancelSyncObservation()
        check(accounts.syncOperation === null && !accounts.syncPollTimer.running, "cancel kept observation alive")
        accounts.invalidateAccount()
        check(accounts.syncStatusRequestId === "" && accounts.gameAccounts.length === 0, "account reset retained state")
        owner.refreshCatalog("")
        owner.catalogStagedBytes = 32 * 1024 * 1024
        deliver(owner, [game(4)], "budget", true)
        check(owner.catalogState === "partial" && owner.catalogNextCursor === "", "resource budget claimed completion")
        owner.catalogGames = [game(1),game(2),game(3),game(4)]
        owner.catalogError = qsTr("Store sync finished, but the library refresh is incomplete. Your saved games are still shown. Retry to load the remaining pages.")
        owner.catalogState = "partial"
        accounts.gameAccountMessage = qsTr("Store sync accepted. Waiting for the store to finish…")
        accounts.syncOperation = {operationId:"visual-sync",provider:"STEAM",phase:"waiting_remote"}
        accounts.gameAccounts = [{provider:"STEAM",label:"Steam",isConnected:true,supportsSync:true,supportsLinking:false,status:"connected",syncedGames:4}]
        if (AppController.route === "game-detail") {
            const patched = game(1)
            patched.variants[0].gfnStatus = "PATCHING"
            patched.variants[0].playStatus = "NOT_PLAYABLE"
            patched.variants[0].stateDetails = {__typename:"VariantGfnAutoPatchingMetadata",historicalEtaMins:18}
            ShellStore.selectedGame = patched
            check(ShellStore.readinessNotice(patched).indexOf("not a completion time") >= 0, "patch estimate was not qualified")
            const modal = namedChild(host, "desktopGameModal")
            check(modal !== null, "production game detail is missing")
            modal.game = patched
            const notice = namedChild(modal, "catalogReadinessNotice")
            check(notice !== null && notice.visible && notice.text.indexOf("not a completion time") >= 0, "production detail did not render the patch notice")
        }
        check(ShellStore.activeSession === null, "catalog work created a session")
        return true
    }
}
