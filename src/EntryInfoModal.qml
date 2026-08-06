import QtQuick
import QtQuick.Controls

// Read-only view of an entry, opened with Tab. Selecting text copies it, the
// way dragging over a field did in the TUI; Tab cycles between the fields.
//
// A pass entry only shows the Title — the format has no URL or Notes of its
// own.
Modal {
    id: root

    property bool passBackend: false
    property var details: ({})

    heading: i18n.t("common.entry_info_title")
    hint: passBackend ? i18n.t("ui.info_footer_pass") : i18n.t("ui.info_footer_full")
    cardWidth: Math.round(600 * s)

    onKeyPressed: function(event) {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Q) {
            root.dismissed();
            event.accepted = true;
        }
    }

    capturesKeys: false

    onShown: titleView.view.forceActiveFocus()

    component InfoField: Column {
        id: fieldColumn

        property string label: ""
        property string value: ""
        property int rows: 1
        property Item nextField: null
        property Item previousField: null
        property alias view: text

        width: parent.width
        spacing: Math.round(6 * root.s)

        Text {
            text: fieldColumn.label
            color: text.activeFocus ? theme.annotation : theme.guidance
            font.family: "iA Writer Mono S"
            font.pixelSize: Math.round(12 * root.s)
        }

        ScrollView {
            width: parent.width
            height: Math.round(22 * root.s) * fieldColumn.rows

            TextEdit {
                id: text
                width: parent.width
                text: fieldColumn.value
                color: theme.base
                readOnly: true
                selectByMouse: true
                selectionColor: theme.annotation
                selectedTextColor: theme.background
                wrapMode: TextEdit.Wrap
                font.family: "iA Writer Mono S"
                font.pixelSize: Math.round(14 * root.s)

                // Copying once the selection settles reproduces the TUI's
                // select-to-copy without firing on every pixel of the drag.
                onSelectedTextChanged: copyTimer.restart()

                Timer {
                    id: copyTimer
                    interval: 250
                    onTriggered: controller.copyText(text.selectedText)
                }

                Keys.onPressed: function(event) {
                    switch (event.key) {
                    case Qt.Key_Escape:
                    case Qt.Key_Q:
                        root.dismissed();
                        break;
                    case Qt.Key_Tab:
                        if (fieldColumn.nextField)
                            fieldColumn.nextField.forceActiveFocus();
                        break;
                    case Qt.Key_Backtab:
                        if (fieldColumn.previousField)
                            fieldColumn.previousField.forceActiveFocus();
                        break;
                    default:
                        return;
                    }
                    event.accepted = true;
                }
            }
        }
    }

    InfoField {
        id: titleView
        label: i18n.t("common.title_word")
        value: root.details.title || ""
        nextField: root.passBackend ? titleView.view : urlView.view
        previousField: root.passBackend ? titleView.view : notesView.view
    }

    InfoField {
        id: urlView
        visible: !root.passBackend
        label: "URL"
        value: root.details.url || ""
        nextField: notesView.view
        previousField: titleView.view
    }

    InfoField {
        id: notesView
        visible: !root.passBackend
        label: i18n.t("common.notes_label")
        value: root.details.notes || ""
        rows: 5
        nextField: titleView.view
        previousField: urlView.view
    }
}
