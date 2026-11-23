import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.components as PlasmaComponents3
import org.kde.kirigami as Kirigami
import org.kde.plasma.plasma5support as Plasma5Support

PlasmoidItem {
    id: root

    property string sessionState: "idle"
    property int elapsedSeconds: 0
    property double bytesWritten: 0
    property double currentFps: 0
    property bool virtualCamActive: false
    property var presetsList: []

    readonly property bool isActive: sessionState !== "idle" && sessionState !== "selecting"
    readonly property bool isRecording: sessionState === "recording" || sessionState === "sharing+recording"
    readonly property bool isPaused: sessionState === "paused"

    switchWidth: Kirigami.Units.gridUnit * 12
    switchHeight: Kirigami.Units.gridUnit * 14

    Plasmoid.status: isActive ? PlasmaCore.Types.ActiveStatus : PlasmaCore.Types.PassiveStatus
    toolTipMainText: i18n("Region to Share")
    toolTipSubText: isRecording
        ? i18n("Recording live (%1)", formatDuration(elapsedSeconds))
        : (isActive ? i18n("Sharing region (%1 FPS)", Math.round(currentFps)) : i18n("Click to share or record a screen region"))

    // Async command bridge to Region to Share D-Bus service / CLI
    Plasma5Support.DataSource {
        id: runner
        engine: "executable"
        connectedSources: []

        onNewData: function(sourceName, data) {
            if (sourceName === statusCmd && data["exit code"] === 0) {
                try {
                    var snap = JSON.parse(data["stdout"]);
                    root.sessionState = snap.state || "idle";
                    root.elapsedSeconds = snap.elapsed_seconds || 0;
                    root.bytesWritten = snap.bytes_written || 0;
                    root.currentFps = snap.current_fps || 0;
                    root.virtualCamActive = !!snap.virtual_cam_active;
                    root.presetsList = snap.presets || [];
                } catch (e) {
                    // Ignore transient partial output
                }
            }
            disconnectSource(sourceName);
        }
    }

    readonly property string statusCmd: "region-to-share status --json"

    function execCmd(cmd) {
        runner.connectSource(cmd);
    }

    function callDbus(methodArgs) {
        execCmd("busctl --user call io.github.solarpush.RegionToShare /io/github/solarpush/RegionToShare io.github.solarpush.RegionToShare1 " + methodArgs);
    }

    function formatDuration(secs) {
        var m = Math.floor(secs / 60);
        var s = secs % 60;
        return (m < 10 ? "0" : "") + m + ":" + (s < 10 ? "0" : "") + s;
    }

    function formatBytes(b) {
        if (b >= 1073741824) return (b / 1073741824).toFixed(2) + " GB";
        if (b >= 1048576) return (b / 1048576).toFixed(1) + " MB";
        if (b >= 1024) return Math.round(b / 1024) + " KB";
        return b + " B";
    }

    Timer {
        interval: root.isActive ? 1000 : 4000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: execCmd(statusCmd)
    }

    compactRepresentation: CompactBadge {}
    fullRepresentation: FullPopup {}
}
