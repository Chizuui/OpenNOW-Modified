import QtQuick
import OpenNOW

QtObject {
    id: fixture
    property var screen: null
    property var picker: null
    property var owner: ShellStore.settingsOwnerState
    property QtObject hdrOutput: QtObject {
        property bool supported: false
        property int outputMode: 0
        property bool chromeRequired: false
        property string status: "Synthetic display capability"
    }
    property QtObject client: QtObject {
        property int serial: 0
        property var calls: []
        function request(method, params, timeout) {
            const id = "language-fixture-" + (++serial)
            calls = calls.concat([{id:id, method:method, params:params, timeout:timeout}])
            return id
        }
        function cancel(id) {
            fixture.check(owner.languageRequestId !== id && owner.colorRequestId !== id,
                "ownership must be cleared before synchronous cancellation")
            fixture.check(owner.acceptFailure(id, "cancelled"), "synchronous cancellation is consumed")
        }
    }

    function check(ok, message) { if (!ok) throw new Error("Language settings: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }
    function reply(languages, status, extra) {
        check(owner.languageRequestId !== "", "language request exists")
        owner.acceptResponse(owner.languageRequestId, Object.assign({languages:languages,
            status:status, source:"overallGfnSupportedLanguages", scopeGeneration:owner.scopeGeneration,
            fetchedAt:Date.now(), expiresAt:Date.now() + 1200000}, extra || {}))
    }
    function run(parent) {
        owner.settingsActive = false
        owner.coreClient = client
        owner.ready = true
        owner.scopeGeneration = 12
        owner.settings = Object.assign({}, owner.settings, {appLanguage:"system", gameLanguage:"es_419",
            keyboardLayout:"ja-JP", colorQuality:"8bit_444", codec:"h265"})
        owner.keyboardLayouts = [{value:"en-US",label:"English (US)",aliases:[]},
            {value:"ja-106",label:"Japanese 106",aliases:["ja-JP","Japanese106"]},
            {value:"es-ES_tradnl",label:"Spanish (traditional)",aliases:["es-ES"]}]
        check(client.calls.length === 0, "metadata is lazy")
        owner.ensureGameLanguages()
        const first = owner.languageRequestId
        owner.ensureGameLanguages()
        check(client.calls.length === 1 && client.calls[0].timeout === 15000, "single flight and bounded deadline")
        check(JSON.stringify(client.calls[0].params) === '{"refresh":false}', "exact metadata RPC parameters")
        owner.scopeGeneration = 13
        check(!owner.acceptResponse(first, {languages:["de_DE"], status:"success", scopeGeneration:12}), "late reply rejected")
        check(owner.gameLanguageItems[0].value === "es_419", "saved offline ID retained exactly")
        owner.ensureGameLanguages()
        reply(["en_US","es_419","zh_Hant_TW"], "success", {cacheHit:true})
        check(owner.languageStatusText.indexOf("cached") >= 0, "cache provenance shown")
        const calls = client.calls.length
        owner.ensureGameLanguages()
        check(client.calls.length === calls, "fresh settings revisit does not refetch")
        owner.ensureGameLanguages(true)
        reply(["en_US","es_419"], "stale", {error:{message:"Synthetic network failure"}})
        check(owner.languageStatusText.indexOf("stale") >= 0, "stale provenance shown")
        owner.ensureGameLanguages(true)
        owner.languageDeadline.triggered()
        check(owner.languageState === "stale" && owner.languageRequestId === "", "timeout retains metadata and clears ownership")
        owner.settings = Object.assign({}, owner.settings, {sessionProxyEnabled:true, sessionProxyUrl:"http://proxy.example.invalid:8080"})
        check(owner.languageState === "idle" && !owner.languageResult.languages, "proxy invalidates metadata")
        owner.ensureGameLanguages()
        reply(["en_US"], "success", {scopeGeneration:11})
        check(owner.languageState === "error", "wrong server scope cannot be accepted")
        owner.ensureGameLanguages(true)
        owner.acceptFailure(owner.languageRequestId, "Synthetic offline failure")
        check(owner.languageState === "error" && owner.settings.gameLanguage === "es_419", "offline failure preserves preference")

        owner.setSetting("gameLanguage", "pt_BR")
        const rejected = owner.settingWrites.gameLanguage.id
        check(owner.settings.gameLanguage === "es_419", "no false optimistic selection")
        owner.setSetting("gameLanguage", "zh_Hant_TW")
        owner.acceptFailure(rejected, "Synthetic rejected write")
        const newer = owner.settingWrites.gameLanguage.id
        check(newer !== rejected, "rapid edits are serialized")
        owner.acceptSettingsChange({key:"gameLanguage",value:"zh_Hant_TW"})
        owner.acceptResponse(newer, {key:"gameLanguage",value:"zh_Hant_TW"})
        owner.acceptFailure(rejected, "Late old rejection")
        check(owner.settings.gameLanguage === "zh_Hant_TW", "old failure cannot undo new success")
        check(owner.settings.appLanguage === "system" && owner.settings.keyboardLayout === "ja-JP", "three persistence keys stay independent")
        check(owner.interfaceLanguageItems[0].value === "system"
            && !owner.interfaceLanguageItems.some(item => item.value === "es_419"), "bundled interface choices are independent")
        for (const failSecond of [false, true]) {
            owner.setSetting("gameLanguage", "zh_Hant_TW")
            const firstWrite = owner.settingWrites.gameLanguage.id
            owner.setSetting("gameLanguage", "fr_FR")
            const requestCount = client.calls.length
            owner.acceptSettingsChange({key:"gameLanguage",value:"zh_Hant_TW"})
            check(client.calls.length === requestCount, "the first event cannot start the queued write")
            owner.acceptResponse(firstWrite, {key:"gameLanguage",value:"zh_Hant_TW"})
            const secondWrite = owner.settingWrites.gameLanguage.id
            check(secondWrite !== firstWrite && client.calls.length === requestCount + 1,
                "the queued write starts only after the first event and response")
            if (failSecond) owner.acceptFailure(secondWrite, "Synthetic second write failure")
            else {
                owner.acceptSettingsChange({key:"gameLanguage",value:"fr_FR"})
                check(owner.settings.gameLanguage === "zh_Hant_TW", "the second event waits for its acknowledgement")
                owner.acceptResponse(secondWrite, {key:"gameLanguage",value:"fr_FR"})
            }
            owner.acceptFailure(firstWrite, "Late old failure")
            check(owner.settings.gameLanguage === (failSecond ? "zh_Hant_TW" : "fr_FR"),
                "queued success or failure keeps the last confirmed value")
        }
        owner.acceptSettingsChange({key:"gameLanguage",value:"it_IT"})
        check(owner.settings.gameLanguage === "it_IT", "legitimate unowned settings events still apply")
        owner.acceptSettingsChange({key:"gameLanguage",value:"zh_Hant_TW"})
        const beforeCoupled = owner.settings
        owner.acceptSettingsChange({key:"themePack",value:"bone",changes:{appTheme:"light",themeAccentOverride:false}})
        check(owner.settings.themePack === "bone" && owner.settings.appTheme === "light"
            && owner.settings.themeAccentOverride === false, "coupled settings events remain atomic")
        owner.settings = beforeCoupled
        owner.setSetting("keyboardLayout", "es-ES_tradnl")
        owner.acceptFailure(owner.settingWrites.keyboardLayout.id, "Synthetic rejected layout")
        check(owner.settings.keyboardLayout === "ja-JP", "rejected layout is not shown as saved")
        check(owner.keyboardLayoutItems[0].value === "ja-JP"
            && owner.keyboardLayoutItems[0].detail.indexOf("ja-106") >= 0, "legacy layout shown truthfully")
        owner.ready = false
        check(owner.settings.gameLanguage === "zh_Hant_TW" && owner.languageState === "idle", "logout/readiness loss preserves saved values")
        owner.ready = true
        check(owner.colorQualityItems.length === 4 && owner.colorQualityItems.every(item => item.disabled), "unknown capability is not support")
        owner.colorRequestId = "old-colors"
        owner.settings = Object.assign({}, owner.settings, {codec:"h264"})
        check(owner.colorRequestId === "" && !owner.acceptResponse("old-colors", {colorQualities:[{value:"8bit_444",disabled:false}]}),
            "codec change cancels old capability choices")
        check(owner.settings.colorQuality === "8bit_444", "codec changes do not coerce saved color")
        owner.colorRequestId = "colors-fixture"
        const colorChoices = [
            {value:"8bit_420",disabled:false}, {value:"8bit_444",disabled:true,reason:"Synthetic backend does not support 4:4:4"},
            {value:"10bit_420",disabled:false}, {value:"10bit_444",disabled:true,reason:"Synthetic backend does not support 4:4:4"}]
        owner.acceptResponse("colors-fixture", {colorQualities:colorChoices})
        check(owner.settings.colorQuality === "8bit_444" && owner.colorQualityItems[1].disabled, "unsupported saved color remains visible")

        owner.settings = Object.assign({}, owner.settings, {gameLanguage:"future_001", codec:"h265", sessionProxyEnabled:false,
            desktopUiScale:Qt.application.arguments.indexOf("--smoke-light-theme") >= 0 ? 1.25 : 1,
            sessionProxyUrl:"", appTheme:Qt.application.arguments.indexOf("--smoke-light-theme") >= 0 ? "light" : "dark"})
        ShellStore.nativeRuntimeReady = true
        ShellStore.nativeRuntimeCapabilities = {protocolVersion:7,
            videoBackends:[{backend:"vaapi",available:true,codecs:[{codec:"h265",available:true,
                colorQualities:["8bit_420","10bit_420"]}]}]}
        if (Qt.application.arguments.indexOf("--language-hdr-invalidation") >= 0) {
            owner.settingsActive = true
            let previous = ""
            for (const supported of [false, true, false]) {
                hdrOutput.supported = supported
                check(owner.nativeHdrOutputSupported === supported, "the production HdrOutput binding tracks display support")
                check(owner.colorRequestId === "", "display changes clear request ownership before cancellation")
                owner.colorRefresh.triggered()
                const request = client.calls[client.calls.length - 1]
                check(request.id === owner.colorRequestId && request.method === "settings.choices.get",
                    "a display change refetches the color descriptors")
                check(request.params.runtimeCapabilities === owner.nativeRuntimeCapabilities
                    && request.params.capabilities === undefined
                    && request.params.runtimeCapabilities.nativeHdrSupported === undefined,
                    "QML forwards native capabilities without authoring display support")
                if (previous !== "") check(!owner.acceptResponse(previous, {colorQualities:colorChoices}),
                    "late previous-display results cannot populate current choices")
                previous = request.id
            }
            owner.acceptResponse(previous, {colorQualities:colorChoices})
            owner.settingsActive = false
        }
        owner.colorDescriptors = colorChoices
        owner.ensureGameLanguages(true)
        const languages = ["en_US","en_GB","de_DE","es_419","es_ES","fr_FR","it_IT","ja_JP","ko_KR","pt_BR","zh_Hant_TW"]
        for (let index = 0; index < 100; ++index) languages.push("future_" + index)
        reply(languages, "stale", {error:{message:"Synthetic offline metadata fixture"},cacheHit:true})
        if (Qt.application.arguments.indexOf("--language-error") >= 0) {
            owner.languageResult = ({})
            owner.languageState = "error"
            owner.languageError = "Synthetic offline metadata fixture"
        }
        screen = find(parent, "desktopSettingsScreen") || find(parent, "consoleSettingsScreen")
        check(screen !== null, "production settings screen exists")
        if (screen.objectName === "desktopSettingsScreen") {
            const colors = Qt.application.arguments.indexOf("--language-colors") >= 0
            picker = find(parent, colors ? "colorQualityChoice" : "gameLanguageChoice")
            check(picker && (colors || find(parent, "keyboardLayoutChoice")), "production choices are present")
            check(JSON.stringify(picker.items) === JSON.stringify(colors ? owner.colorQualityItems : owner.gameLanguageItems), "desktop shares exact choices")
            check(JSON.stringify(screen.colorQualityItems()) === JSON.stringify(owner.colorQualityItems), "desktop shares color choices")
            picker.expanded = true
            if (Qt.application.arguments.indexOf("--language-keyboard-selection") >= 0) {
                const filter = picker.children.find(child => child.placeholderText === picker.filterPlaceholder)
                check(filter, "production filter exists")
                filter.text = "es_419"
            }
        } else {
            const list = find(screen, "consoleSettingsList")
            const colorView = Qt.application.arguments.indexOf("--language-colors") >= 0
            const index = screen.settingsModel().findIndex(item => item.key === (colorView ? "colorQuality" : "gameLanguage"))
            check(list && index >= 0, "production console row exists")
            list.currentIndex = index
            list.positionViewAtIndex(index, ListView.Center)
            const row = screen.descriptorChoice("Game language", owner.gameLanguageDescription, "gameLanguage", owner.gameLanguageItems)
            check(JSON.stringify(row.values) === JSON.stringify(owner.gameLanguageItems.map(item => item.value)), "console shares exact game IDs")
            const colors = screen.descriptorChoice("Color quality", owner.colorDescription, "colorQuality", owner.colorQualityItems)
            check(JSON.stringify(colors.disabledValues) === JSON.stringify(owner.colorQualityItems.filter(item => item.disabled).map(item => item.value)), "console shares capability decisions")
            screen.openChoices(Qt.application.arguments.indexOf("--language-colors") >= 0 ? colors : row)
            if (colorView) check(screen.dropdownPanelHeight >= 4 * 64 + 91, "all four color rows fit with their reasons")
        }
        ShellStore.lastError = ""
        return true
    }

    function verify() {
        check(picker ? !picker.expanded : !screen.dropdownOpen, "keyboard interaction closes the real production picker")
        if (Qt.application.arguments.indexOf("--language-keyboard-selection") >= 0) {
            check(owner.settingWrites.gameLanguage && owner.settingWrites.gameLanguage.value === "es_419",
                "Tab and Enter select the exact filtered language ID")
            check(owner.settings.gameLanguage === "future_001", "keyboard choice waits for persistence")
            owner.acceptResponse(owner.settingWrites.gameLanguage.id, {value:"es_419"})
            check(owner.settings.gameLanguage === "es_419", "keyboard choice confirms after persistence")
        }
        if (Qt.application.arguments.indexOf("--screenshot") >= 0
                && Qt.application.arguments.indexOf("--language-error") < 0) {
            if (picker) {
                picker.expanded = true
                if (Qt.application.arguments.indexOf("--language-colors") >= 0) Qt.callLater(() => {
                    const content = find(screen, "desktopSettingsContent")
                    content.contentY = Math.min(content.contentHeight - content.height,
                        picker.mapToItem(content.contentItem, 0, 0).y)
                })
            }
            else {
                const colors = Qt.application.arguments.indexOf("--language-colors") >= 0
                screen.openChoices(screen.descriptorChoice(colors ? "Color quality" : "Game language",
                    colors ? owner.colorDescription : owner.gameLanguageDescription,
                    colors ? "colorQuality" : "gameLanguage", colors ? owner.colorQualityItems : owner.gameLanguageItems))
            }
        }
        return true
    }
}
