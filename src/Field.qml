import QtQuick
import QtQuick.Controls

// A labelled single-line field. The label takes the annotation colour while
// the field has focus, which is how the TUI marked the active box.
Item {
    id: root

    property string label: ""
    property alias text: input.text
    property alias echoMode: input.echoMode
    property alias placeholderText: input.placeholderText
    property alias field: input
    readonly property bool active: input.activeFocus

    signal submitted()
    signal cancelled()
    signal nextRequested()
    signal previousRequested()
    signal listNext()
    signal listPrevious()

    readonly property real s: systemTheme.textScale

    implicitHeight: labelText.height + Math.round(6 * s) + input.height
    height: implicitHeight

    Text {
        id: labelText
        anchors.top: parent.top
        text: root.label
        color: root.active ? theme.annotation : theme.guidance
        font.family: "iA Writer Mono S"
        font.pixelSize: Math.round(12 * root.s)
    }

    TextField {
        id: input
        anchors.top: labelText.bottom
        anchors.topMargin: Math.round(6 * root.s)
        width: parent.width
        height: Math.round(34 * root.s)
        color: theme.base
        placeholderTextColor: theme.guidance
        selectionColor: theme.selection
        selectedTextColor: theme.base
        font.family: "iA Writer Mono S"
        font.pixelSize: Math.round(15 * root.s)
        leftPadding: 0
        rightPadding: 0

        background: Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: Math.max(1, Math.round(root.s))
            color: root.active ? theme.annotation : theme.guidance
            opacity: root.active ? 1.0 : 0.4
        }

        Keys.onPressed: function(event) {
            if (event.modifiers & Qt.ControlModifier) {
                // Readline-style navigation for the suggestion dropdowns:
                // this is a text field, so j/k cannot navigate here.
                if (event.key === Qt.Key_N) {
                    root.listNext();
                    event.accepted = true;
                } else if (event.key === Qt.Key_P) {
                    root.listPrevious();
                    event.accepted = true;
                }
                return;
            }

            switch (event.key) {
            case Qt.Key_Escape:
                root.cancelled();
                break;
            case Qt.Key_Return:
            case Qt.Key_Enter:
                root.submitted();
                break;
            case Qt.Key_Tab:
                root.nextRequested();
                break;
            case Qt.Key_Backtab:
                root.previousRequested();
                break;
            case Qt.Key_Down:
                root.listNext();
                break;
            case Qt.Key_Up:
                root.listPrevious();
                break;
            default:
                return;
            }
            event.accepted = true;
        }
    }
}
