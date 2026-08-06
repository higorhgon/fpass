import QtQuick

// Initialises a new pass store: where it lives, and which of the GPG keys
// already in the keyring it encrypts to. fpass never generates GPG keys.
Modal {
    id: root

    property string errorText: ""
    property bool busy: false
    property var keys: []

    signal submitted(string directory, string keyId)

    heading: i18n.t("db_app.new_pass_store_title")
    hint: errorText.length > 0 ? errorText : i18n.t("common.footer_create_pass")
    hintColor: errorText.length > 0 ? theme.alertError : theme.guidance
    accentColor: errorText.length > 0 ? theme.alertError : theme.alertInfo
    cardWidth: Math.round(560 * s)

    capturesKeys: false

    onShown: {
        root.keys = controller.gpgKeys();
        directoryField.text = controller.defaultPassStoreDirectory();
        refreshSuggestions();
        keys.currentIndex = 0;
        directoryField.field.forceActiveFocus();
    }

    function refreshSuggestions() {
        suggestions.model = controller.directorySuggestions(directoryField.text);
        suggestions.currentIndex = 0;
    }

    // Completes the highlighted folder and leaves the trailing slash in
    // place, so the next segment can be completed straight away.
    function acceptSuggestion() {
        if (suggestions.model.length === 0)
            return false;

        directoryField.text = suggestions.model[suggestions.currentIndex] + "/";
        refreshSuggestions();
        return true;
    }

    function submit() {
        if (root.keys.length === 0)
            return;
        root.submitted(directoryField.text, root.keys[keys.currentIndex].id);
    }

    Field {
        id: directoryField
        width: parent.width
        label: i18n.t("db_app.dir_label")
        enabled: !root.busy

        onTextChanged: root.refreshSuggestions()
        onCancelled: root.dismissed()
        onListNext: suggestions.next()
        onListPrevious: suggestions.previous()
        onNextRequested: keyList.forceActiveFocus()
        onPreviousRequested: keyList.forceActiveFocus()
        // Enter completes the highlighted folder when there is one; Tab is
        // always available to jump straight to the key list.
        onSubmitted: if (!root.acceptSuggestion()) keyList.forceActiveFocus()
    }

    SuggestionList {
        id: suggestions
        width: parent.width
        labelFor: function(item) { return String(item).split("/").pop(); }
        onPicked: root.acceptSuggestion()
    }

    Text {
        width: parent.width
        text: i18n.t("db_app.gpg_key_label")
        color: keyList.activeFocus ? theme.annotation : theme.guidance
        font.family: "iA Writer Mono S"
        font.pixelSize: Math.round(12 * root.s)
    }

    Text {
        width: parent.width
        visible: root.keys.length === 0
        text: i18n.t("db_app.no_gpg_key_found")
        color: theme.alertWarn
        wrapMode: Text.WordWrap
        font.family: "iA Writer Mono S"
        font.pixelSize: Math.round(13 * root.s)
    }

    FocusScope {
        id: keyList
        width: parent.width
        height: keys.height
        visible: root.keys.length > 0

        SuggestionList {
            id: keys
            width: parent.width
            visibleRows: 5
            model: root.keys
            labelFor: function(item) { return item.id + "  " + item.identity; }
            onPicked: root.submit()
        }

        Keys.onPressed: function(event) {
            if (event.modifiers & Qt.ControlModifier) {
                if (event.key === Qt.Key_N) {
                    keys.next();
                    event.accepted = true;
                } else if (event.key === Qt.Key_P) {
                    keys.previous();
                    event.accepted = true;
                }
                return;
            }

            switch (event.key) {
            case Qt.Key_Down:
            case Qt.Key_J:
                keys.next();
                break;
            case Qt.Key_Up:
            case Qt.Key_K:
                keys.previous();
                break;
            case Qt.Key_Tab:
            case Qt.Key_Backtab:
                directoryField.field.forceActiveFocus();
                break;
            case Qt.Key_Return:
            case Qt.Key_Enter:
                root.submit();
                break;
            case Qt.Key_Escape:
                root.dismissed();
                break;
            default:
                return;
            }
            event.accepted = true;
        }
    }
}
