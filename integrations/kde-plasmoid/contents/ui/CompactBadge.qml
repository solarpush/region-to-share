import QtQuick
import QtQuick.Layouts
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.components as PlasmaComponents3
import org.kde.kirigami as Kirigami

MouseArea {
    id: compactRoot

    Layout.minimumWidth: row.implicitWidth + Kirigami.Units.smallSpacing * 2
    Layout.preferredWidth: Layout.minimumWidth
    Layout.fillHeight: true

    acceptedButtons: Qt.LeftButton | Qt.MiddleButton
    hoverEnabled: true

    onClicked: function(mouse) {
        if (mouse.button === Qt.MiddleButton) {
            if (root.isActive) {
                root.callDbus("ToggleRecord");
            } else {
                root.callDbus("StartShare bsb true \"\" false");
            }
        } else {
            root.expanded = !root.expanded;
        }
    }

    RowLayout {
        id: row
        anchors.centerIn: parent
        spacing: Kirigami.Units.smallSpacing

        Item {
            Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
            Layout.preferredHeight: Kirigami.Units.iconSizes.smallMedium

            Kirigami.Icon {
                anchors.fill: parent
                source: root.isRecording ? "media-record" : "camera-video-symbolic"
                color: root.isRecording ? "#ff4d5e" : (root.isActive ? "#2dd4bf" : Kirigami.Theme.textColor)
            }

            Rectangle {
                visible: root.isActive
                width: 8
                height: 8
                radius: 4
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                color: root.isRecording ? "#ff4d5e" : (root.isPaused ? "#f59e0b" : "#2dd4bf")
                border.color: Kirigami.Theme.backgroundColor
                border.width: 1
            }
        }

        PlasmaComponents3.Label {
            visible: root.isActive
            text: root.isRecording
                ? root.formatDuration(root.elapsedSeconds)
                : (Math.round(root.currentFps) + " FPS")
            font.pixelSize: Math.round(Kirigami.Theme.smallFont.pixelSize)
            font.family: "monospace"
            color: root.isRecording ? "#ff4d5e" : Kirigami.Theme.textColor
        }
    }
}
