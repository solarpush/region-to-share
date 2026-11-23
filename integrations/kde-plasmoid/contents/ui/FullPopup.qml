import QtQuick
import QtQuick.Layouts
import org.kde.plasma.components as PlasmaComponents3
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: popupRoot
    Layout.minimumWidth: Kirigami.Units.gridUnit * 16
    Layout.minimumHeight: Kirigami.Units.gridUnit * 16
    spacing: Kirigami.Units.smallSpacing

    // Header
    RowLayout {
        Layout.fillWidth: true
        Kirigami.Heading {
            level: 3
            text: i18n("Region to Share")
            Layout.fillWidth: true
        }
        PlasmaComponents3.ToolButton {
            icon.name: "configure"
            PlasmaComponents3.ToolTip.text: i18n("Open Region to Share")
            PlasmaComponents3.ToolTip.visible: hovered
            onClicked: {
                root.callDbus("ShowWindow");
                root.expanded = false;
            }
        }
    }

    // Active Live Session Banner
    Rectangle {
        visible: root.isActive
        Layout.fillWidth: true
        implicitHeight: activeCol.implicitHeight + Kirigami.Units.largeSpacing
        radius: Kirigami.Units.cornerRadius
        color: root.isRecording ? Qt.rgba(1, 0.3, 0.37, 0.16) : Qt.rgba(0.18, 0.83, 0.75, 0.16)
        border.color: root.isRecording ? "#ff4d5e" : "#2dd4bf"
        border.width: 1

        ColumnLayout {
            id: activeCol
            anchors.fill: parent
            anchors.margins: Kirigami.Units.smallSpacing
            spacing: Kirigami.Units.smallSpacing

            PlasmaComponents3.Label {
                text: root.isRecording
                    ? i18n("● REC %1 · %2", root.formatDuration(root.elapsedSeconds), root.formatBytes(root.bytesWritten))
                    : i18n("● LIVE · %1 FPS", Math.round(root.currentFps))
                font.bold: true
            }

            RowLayout {
                Layout.fillWidth: true
                PlasmaComponents3.Button {
                    text: root.isRecording ? i18n("Stop Rec") : i18n("Record")
                    icon.name: root.isRecording ? "media-playback-stop" : "media-record"
                    onClicked: root.callDbus("ToggleRecord")
                }
                PlasmaComponents3.Button {
                    visible: root.isRecording
                    text: root.isPaused ? i18n("Resume") : i18n("Pause")
                    icon.name: root.isPaused ? "media-playback-start" : "media-playback-pause"
                    onClicked: root.callDbus(root.isPaused ? "Resume" : "Pause")
                }
                PlasmaComponents3.Button {
                    text: i18n("Stop")
                    icon.name: "process-stop"
                    onClicked: {
                        root.callDbus("Stop");
                        root.expanded = false;
                    }
                }
            }
        }
    }

    // Primary 1-Click Actions
    PlasmaComponents3.Button {
        Layout.fillWidth: true
        text: i18n("Share Region")
        icon.name: "transform-crop"
        onClicked: {
            root.callDbus("StartShare bsb false \"\" false");
            root.expanded = false;
        }
    }

    PlasmaComponents3.Button {
        Layout.fillWidth: true
        text: i18n("Record Region Live (MKV / MP4)")
        icon.name: "media-record"
        onClicked: {
            root.callDbus("StartRecord bssb false \"\" \"\" true");
            root.expanded = false;
        }
    }

    PlasmaComponents3.Button {
        Layout.fillWidth: true
        text: i18n("Virtual Camera (Webcam)")
        icon.name: "camera-web"
        onClicked: {
            root.callDbus("StartShare bsb false \"\" true");
            root.expanded = false;
        }
    }

    PlasmaComponents3.Button {
        Layout.fillWidth: true
        text: i18n("Quick Snapshot to Clipboard")
        icon.name: "camera-photo"
        onClicked: {
            root.callDbus("TakeSnapshot bs true \"\"");
            root.expanded = false;
        }
    }

    // Saved Presets
    Kirigami.Separator {
        Layout.fillWidth: true
        visible: root.presetsList.length > 0
    }

    PlasmaComponents3.Label {
        visible: root.presetsList.length > 0
        text: i18n("Saved Presets")
        font.bold: true
        opacity: 0.75
    }

    Repeater {
        model: root.presetsList
        delegate: RowLayout {
            Layout.fillWidth: true
            PlasmaComponents3.Label {
                text: modelData.name + " (" + modelData.width + "×" + modelData.height + ")"
                elide: Text.ElideRight
                Layout.fillWidth: true
            }
            PlasmaComponents3.ToolButton {
                icon.name: "media-playback-start"
                onClicked: {
                    root.callDbus("StartShare bsb false \"" + modelData.name + "\" false");
                    root.expanded = false;
                }
            }
            PlasmaComponents3.ToolButton {
                icon.name: "media-record"
                onClicked: {
                    root.callDbus("StartRecord bssb false \"" + modelData.name + "\" \"\" true");
                    root.expanded = false;
                }
            }
        }
    }
}
