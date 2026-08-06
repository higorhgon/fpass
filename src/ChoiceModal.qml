import QtQuick

// A short list of mutually exclusive options, navigated with j/k or the
// arrows and confirmed with Enter. Used for "which kind of database?" and
// the right-click actions menu.
Modal {
    id: root

    property var options: []
    property int selected: 0

    signal chosen(int index)

    onKeyPressed: function(event) {
        switch (event.key) {
        case Qt.Key_Escape:
            root.dismissed();
            break;
        case Qt.Key_Down:
        case Qt.Key_J:
            root.selected = (root.selected + 1) % root.options.length;
            break;
        case Qt.Key_Up:
        case Qt.Key_K:
            root.selected = (root.selected + root.options.length - 1) % root.options.length;
            break;
        case Qt.Key_Return:
        case Qt.Key_Enter:
            root.chosen(root.selected);
            break;
        default:
            return;
        }
        event.accepted = true;
    }

    Repeater {
        model: root.options

        Rectangle {
            required property int index
            required property var modelData

            width: parent.width
            height: Math.round(30 * root.s)
            radius: Math.round(4 * root.s)
            color: root.selected === index ? theme.selection : "transparent"

            Text {
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: parent.left
                anchors.leftMargin: Math.round(10 * root.s)
                text: modelData
                color: theme.base
                font.pixelSize: Math.round(14 * root.s)
            }

            HoverHandler {
                onHoveredChanged: if (hovered) root.selected = index
            }

            TapHandler {
                onTapped: root.chosen(index)
            }
        }
    }
}
