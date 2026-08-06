import QtQuick

// The full shortcut list, opened with Ctrl+?. Each page supplies its own
// sections as [{ title, items: [[keys, description], …] }].
Modal {
    id: root

    property var sections: []

    heading: i18n.t("common.shortcuts_title")
    hint: i18n.t("common.footer_close")
    cardWidth: Math.round(560 * s)

    onKeyPressed: function(event) {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Q
                || event.key === Qt.Key_Question) {
            root.dismissed();
            event.accepted = true;
        }
    }

    Repeater {
        model: root.sections

        Column {
            required property var modelData

            width: parent.width
            spacing: Math.round(4 * root.s)

            Text {
                text: modelData.title
                color: theme.title
                font.bold: true
                font.pixelSize: Math.round(13 * root.s)
                bottomPadding: Math.round(3 * root.s)
            }

            Repeater {
                model: modelData.items

                Row {
                    required property var modelData

                    spacing: Math.round(10 * root.s)

                    Text {
                        width: Math.round(150 * root.s)
                        text: modelData[0]
                        color: theme.annotation
                        font.pixelSize: Math.round(13 * root.s)
                    }

                    Text {
                        text: modelData[1]
                        color: theme.base
                        font.pixelSize: Math.round(13 * root.s)
                    }
                }
            }
        }
    }
}
