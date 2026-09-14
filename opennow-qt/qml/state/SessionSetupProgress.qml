import QtQuick

QtObject {
    id: root
    required property var session

    readonly property int setupStep: session && typeof session.seatSetupStep === "number"
        && Number.isInteger(session.seatSetupStep) && session.seatSetupStep >= 0
        && session.seatSetupStep <= 6 ? session.seatSetupStep : -1
    readonly property int queuePosition: {
        const position = Number(session && session.queuePosition || 0)
        return Number.isFinite(position) ? Math.min(2147483647, Math.max(0, Math.floor(position))) : 0
    }
    readonly property bool queued: setupStep !== 5 && setupStep !== 6
        && (setupStep === 1 || queuePosition > 0)
    readonly property string title: {
        if (setupStep === 5) return qsTr("Cleaning up your previous session")
        if (setupStep === 6) return qsTr("Waiting for cloud storage")
        if (queued) return queuePosition > 0
            ? qsTr("Queue position %1").arg(queuePosition) : qsTr("Waiting for an available rig")
        if (setupStep === 0) return qsTr("Connecting to GeForce NOW")
        if (setupStep >= 2 && setupStep <= 4) return qsTr("Configuring your cloud gaming rig")
        return qsTr("Preparing your game")
    }
    readonly property string detail: {
        if (setupStep === 5) return qsTr("GeForce NOW is releasing resources from your previous session before starting this one.")
        if (setupStep === 6) return qsTr("GeForce NOW is waiting for session storage to become available.")
        if (queued) return qsTr("Waiting for an available rig. Your session will start automatically.")
        if (setupStep === 0) return qsTr("GeForce NOW is connecting your session to a cloud gaming server.")
        if (setupStep >= 2 && setupStep <= 4) return qsTr("Your rig has been allocated. GeForce NOW is configuring it for your game.")
        return qsTr("Your session will start automatically when it is ready.")
    }
}
