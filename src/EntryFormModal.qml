import QtQuick
import QtQuick.Controls

// Add or edit an entry. A pass entry only exposes Group, Title and Password —
// the pass format is far less structured than KeePassXC's, and the fields it
// does not model are preserved untouched through `extra`.
Modal {
    id: root

    property bool isEdit: false
    property bool passBackend: false
    property var fields: ({})

    signal submitted(var payload)

    heading: isEdit ? i18n.t("common.edit_entry_title") : i18n.t("common.new_entry_title")
    hint: passBackend ? i18n.t("ui.footer_form_pass") : i18n.t("ui.footer_form_full")
    cardWidth: Math.round(560 * s)

    capturesKeys: false

    onShown: {
        groupField.text = root.fields.group || "";
        titleField.text = root.fields.title || "";
        usernameField.text = root.fields.username || "";
        passwordField.text = root.fields.password || "";
        urlField.text = root.fields.url || "";
        notesArea.text = root.fields.notes || "";
        refreshGroups();

        // Group and title already came from the path, so editing starts on
        // the password — the field that actually changes most often.
        if (root.isEdit)
            passwordField.field.forceActiveFocus();
        else
            groupField.field.forceActiveFocus();
    }

    function refreshGroups() {
        groupSuggestions.model = controller.matchingGroups(groupField.text);
        groupSuggestions.currentIndex = 0;
    }

    function acceptGroupSuggestion() {
        if (groupSuggestions.model.length === 0)
            return false;

        groupField.text = groupSuggestions.model[groupSuggestions.currentIndex];
        titleField.field.forceActiveFocus();
        return true;
    }

    function submit() {
        root.submitted({
            "isEdit": root.isEdit,
            "originalPath": root.fields.originalPath || "",
            "group": groupField.text,
            "title": titleField.text,
            "username": usernameField.text,
            "password": passwordField.text,
            "url": urlField.text,
            "notes": notesArea.text,
            "extra": root.fields.extra || []
        });
    }

    // Tab order skips the fields the pass backend does not show.
    function focusNext(from) {
        const order = root.passBackend
            ? [groupField, titleField, passwordField]
            : [groupField, titleField, usernameField, passwordField, urlField, notesArea];
        const index = order.indexOf(from);
        const target = order[(index + 1) % order.length];
        if (target === notesArea)
            notesArea.forceActiveFocus();
        else
            target.field.forceActiveFocus();
    }

    function focusPrevious(from) {
        const order = root.passBackend
            ? [groupField, titleField, passwordField]
            : [groupField, titleField, usernameField, passwordField, urlField, notesArea];
        const index = order.indexOf(from);
        const target = order[(index + order.length - 1) % order.length];
        if (target === notesArea)
            notesArea.forceActiveFocus();
        else
            target.field.forceActiveFocus();
    }

    Field {
        id: groupField
        width: parent.width
        label: i18n.t("common.group_label")

        onTextChanged: root.refreshGroups()
        onCancelled: root.dismissed()
        onListNext: groupSuggestions.next()
        onListPrevious: groupSuggestions.previous()
        onNextRequested: root.focusNext(groupField)
        onPreviousRequested: root.focusPrevious(groupField)
        onSubmitted: if (!root.acceptGroupSuggestion()) root.submit()
    }

    SuggestionList {
        id: groupSuggestions
        width: parent.width
        onPicked: root.acceptGroupSuggestion()
    }

    Field {
        id: titleField
        width: parent.width
        label: i18n.t("common.title_label")

        onCancelled: root.dismissed()
        onSubmitted: root.submit()
        onNextRequested: root.focusNext(titleField)
        onPreviousRequested: root.focusPrevious(titleField)
    }

    Field {
        id: usernameField
        width: parent.width
        visible: !root.passBackend
        label: i18n.t("common.username_label")

        onCancelled: root.dismissed()
        onSubmitted: root.submit()
        onNextRequested: root.focusNext(usernameField)
        onPreviousRequested: root.focusPrevious(usernameField)
    }

    Field {
        id: passwordField
        width: parent.width
        label: i18n.t("common.password_label")
        echoMode: revealPassword.checked ? TextInput.Normal : TextInput.Password

        onCancelled: root.dismissed()
        onSubmitted: root.submit()
        onNextRequested: root.focusNext(passwordField)
        onPreviousRequested: root.focusPrevious(passwordField)
    }

    CheckBox {
        id: revealPassword
        text: i18n.t("ui.show_password")
        font.family: "iA Writer Mono S"
        font.pixelSize: Math.round(12 * root.s)
        // Never steals the Tab order from the fields around it.
        focusPolicy: Qt.NoFocus

        indicator: Rectangle {
            implicitWidth: Math.round(16 * root.s)
            implicitHeight: Math.round(16 * root.s)
            y: (revealPassword.height - height) / 2
            radius: Math.round(3 * root.s)
            color: "transparent"
            border.width: Math.max(1, Math.round(root.s))
            border.color: revealPassword.checked ? theme.annotation : theme.guidance

            Rectangle {
                anchors.centerIn: parent
                width: parent.width / 2
                height: parent.height / 2
                radius: Math.round(2 * root.s)
                visible: revealPassword.checked
                color: theme.annotation
            }
        }

        contentItem: Text {
            text: revealPassword.text
            color: theme.guidance
            leftPadding: revealPassword.indicator.width + Math.round(6 * root.s)
            verticalAlignment: Text.AlignVCenter
            font: revealPassword.font
        }
    }

    Field {
        id: urlField
        width: parent.width
        visible: !root.passBackend
        label: i18n.t("common.url_label")

        onCancelled: root.dismissed()
        onSubmitted: root.submit()
        onNextRequested: root.focusNext(urlField)
        onPreviousRequested: root.focusPrevious(urlField)
    }

    Column {
        width: parent.width
        visible: !root.passBackend
        spacing: Math.round(6 * root.s)

        Text {
            text: i18n.t("common.notes_label")
            color: notesArea.activeFocus ? theme.annotation : theme.guidance
            font.family: "iA Writer Mono S"
            font.pixelSize: Math.round(12 * root.s)
        }

        ScrollView {
            width: parent.width
            height: Math.round(80 * root.s)

            TextArea {
                id: notesArea
                color: theme.base
                selectionColor: theme.selection
                selectedTextColor: theme.base
                wrapMode: TextArea.Wrap
                font.family: "iA Writer Mono S"
                font.pixelSize: Math.round(14 * root.s)

                background: Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: Math.max(1, Math.round(root.s))
                    color: notesArea.activeFocus ? theme.annotation : theme.guidance
                    opacity: notesArea.activeFocus ? 1.0 : 0.4
                }

                // Enter inserts a line break here, as it did in the TUI;
                // Ctrl+Enter is what saves from inside the notes.
                Keys.onPressed: function(event) {
                    if (event.key === Qt.Key_Escape) {
                        root.dismissed();
                        event.accepted = true;
                    } else if (event.key === Qt.Key_Tab) {
                        root.focusNext(notesArea);
                        event.accepted = true;
                    } else if (event.key === Qt.Key_Backtab) {
                        root.focusPrevious(notesArea);
                        event.accepted = true;
                    } else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                               && (event.modifiers & Qt.ControlModifier)) {
                        root.submit();
                        event.accepted = true;
                    }
                }
            }
        }
    }
}
