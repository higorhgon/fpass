import QtQuick

// Creates a KeePassXC database in ~/.config/fpass/databases: a filename and
// the master password to protect it with.
Modal {
    id: root

    property string errorText: ""
    property bool busy: false

    signal submitted(string name, string password)

    heading: i18n.t("db_app.create_db_title")
    hint: errorText.length > 0 ? errorText : i18n.t("common.footer_navigate_create")
    hintColor: errorText.length > 0 ? theme.alertError : theme.guidance
    accentColor: errorText.length > 0 ? theme.alertError : theme.alertInfo

    capturesKeys: false

    onShown: {
        nameField.text = "";
        passwordField.text = "";
        nameField.field.forceActiveFocus();
    }

    function submit() {
        if (nameField.text.trim().length > 0)
            root.submitted(nameField.text, passwordField.text);
    }

    Field {
        id: nameField
        width: parent.width
        label: i18n.t("db_app.filename_label")
        enabled: !root.busy

        // Enter on the name moves on to the password rather than creating a
        // database with an empty one.
        onSubmitted: passwordField.field.forceActiveFocus()
        onNextRequested: passwordField.field.forceActiveFocus()
        onPreviousRequested: passwordField.field.forceActiveFocus()
        onCancelled: root.dismissed()
    }

    Field {
        id: passwordField
        width: parent.width
        label: i18n.t("db_app.db_password_label")
        echoMode: TextInput.Password
        enabled: !root.busy

        onSubmitted: root.submit()
        onNextRequested: nameField.field.forceActiveFocus()
        onPreviousRequested: nameField.field.forceActiveFocus()
        onCancelled: root.dismissed()
    }
}
