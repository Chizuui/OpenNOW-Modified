import QtQuick
import OpenNOW

QtObject {
    property QtObject client: QtObject {
        property string state: "ready"
        property string lastError: ""
        property var calls: []
        signal responseReceived(string id, var result)
        signal requestFailed(string id, string code, string message)
        signal eventReceived(string name, var payload)
        function markUiReady() {}
        function logShellDiagnostic(message) {}
        function request(method, params, timeout) {
            const id = "queue-acceptance-" + (calls.length + 1)
            calls = calls.concat([{id:id, method:method, params:params}])
            return id
        }
        function cancel(id) { return true }
    }
    function check(ok, message) { if (!ok) throw new Error("Queue selector: " + message) }
    function count(method) { return client.calls.filter(item => item.method === method).length }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found) return found
        }
        return null
    }
    readonly property var game: {
        return {id:"queue-fixture", title:"Cyberpunk 2077", selectedVariantIndex:0,
            variants:[{id:"12345", store:"STEAM", inLibrary:true, gfnStatus:"AVAILABLE"}]}
    }
    readonly property var locations: [
        {zoneId:"NP-LAX-03", title:"Southern California", region:"US Southwest", queuePosition:12,
            etaMs:192000, pingMs:24, lastUpdated:1789623725, streamingBaseUrl:"https://np-lax-03.cloudmatchbeta.nvidiagrid.net/", alternateCount:1},
        {zoneId:"NP-DAL-04", title:"Dallas", region:"US Central", queuePosition:3,
            etaMs:138000, pingMs:62, lastUpdated:1789623725, streamingBaseUrl:"https://np-dal-04.cloudmatchbeta.nvidiagrid.net/", alternateCount:0},
        {zoneId:"NP-ASH-04", title:"Ashburn", region:"US East", queuePosition:18,
            etaMs:240000, pingMs:83, lastUpdated:1789623725, streamingBaseUrl:"https://np-ash-04.cloudmatchbeta.nvidiagrid.net/", alternateCount:0},
        {zoneId:"NP-PAR-01", title:"Paris", region:"EU Southwest", queuePosition:0,
            etaMs:null, pingMs:null, lastUpdated:1789623725, streamingBaseUrl:"https://np-par-01.cloudmatchbeta.nvidiagrid.net/", alternateCount:0}
    ]
    function begin() {
        ShellStore.selectedGame = game
        ShellStore.launchSelectedGame(false)
        check(ShellStore.launchInspectRequestId !== "", "launch inspection did not start")
        client.responseReceived(ShellStore.launchInspectRequestId, {
            appId:game.id, variantId:"12345", game:game, decision:{status:"ready"},
            scope:ShellStore.catalogOwnerState.authScope
        })
    }
    function respond() {
        const state = ShellStore.queueSelector
        check(state.opened && state.requestId !== "", "free launch did not request queues")
        client.responseReceived(state.requestId, {locations:locations, recommendedZoneId:"NP-LAX-03"})
    }
    function run(host) {
        const scaled = Qt.application.arguments.indexOf("--queue-selector-large") >= 0
        ShellStore.settings = Object.assign({}, ShellStore.settings, {
            onboardingCompleted:true, hideQueueSelector:false, desktopUiScale:scaled ? 1.4 : 1
        })
        ShellStore.authGeneration = 7
        ShellStore.authSession = {user:{userId:"queue-fixture",displayName:"Queue fixture",membershipTier:"FREE"},
            provider:{idpId:"nvidia-fixture",code:"NVIDIA"}}
        ShellStore.subscription = {membershipTier:"FREE", isGamePlayAllowed:true}
        ShellStore.subscriptionRequestId = ""
        ShellStore.nativeRuntimeReady = true
        ShellStore.activeSession = null
        AppController.navigate("home")
        begin()
        respond()
        check(count("session.remote.list") === 0 && count("session.create") === 0,
            "session started before a region was chosen")
        if (Qt.application.arguments.indexOf("--queue-selector-preview") >= 0) return true
        ShellStore.queueSelector.choose("NP-DAL-04")
        const sent = client.calls.find(item => item.id === ShellStore.remoteSessionsRequestId)
        check(sent && sent.params.zone === "NP-DAL-04"
            && sent.params.streamingBaseUrl === locations[1].streamingBaseUrl,
            "selected route did not reach session discovery")
        check(!ShellStore.settings.region, "one-launch route changed the saved region")
        ShellStore.remoteSessionsRequestId = ""
        ShellStore.pendingLaunchParams = null
        ShellStore.streamState = "idle"
        AppController.navigate("home")
        ShellStore.subscription = {membershipTier:"ULTIMATE", isGamePlayAllowed:true}
        const before = count("queue.servers.list")
        begin()
        check(!ShellStore.queueSelector.opened && count("queue.servers.list") === before,
            "paid membership opened the free-tier selector")
        check(ShellStore.remoteSessionsRequestId !== "", "paid launch did not continue")
        ShellStore.remoteSessionsRequestId = ""
        ShellStore.pendingLaunchParams = null
        ShellStore.streamState = "idle"
        ShellStore.subscription = {membershipTier:"FREE", isGamePlayAllowed:true}
        ShellStore.settings = Object.assign({}, ShellStore.settings, {hideQueueSelector:true})
        begin()
        check(!ShellStore.queueSelector.opened && count("queue.servers.list") === before,
            "opted-out membership opened the selector")
        ShellStore.remoteSessionsRequestId = ""
        ShellStore.pendingLaunchParams = null
        ShellStore.streamState = "idle"
        ShellStore.settings = Object.assign({}, ShellStore.settings, {hideQueueSelector:false})
        AppController.navigate("home")
        begin()
        respond()
        return true
    }
    function verifyRendered(host) {
        check(ShellStore.queueSelector.opened, "selector closed before rendering")
        for (const name of ["queueSelectorDontShow", "queueSelectorCredit", "queueSelectorDefault", "queueSelectorPlay"]) {
            const control = find(host, name)
            check(control && control.visible, name + " was not visible")
            const edge = control.mapToItem(host, control.width, control.height)
            check(edge.x <= host.width && edge.y <= host.height, name + " exceeded the window")
        }
        return true
    }
}
