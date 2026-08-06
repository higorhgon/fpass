import QtQuick

// Asks for the master password (KeePassXC) or the GPG passphrase (pass) of
// the database about to be opened.
Modal {
    id: root

    property string databaseName: ""
    property string errorText: ""
    property bool busy: false

    signal submitted(string password)

    accentColor: errorText.length > 0 ? theme.alertError : theme.alertInfo
    heading: i18n.t("db_app.unlock_title", { "db": databaseName })
    hint: busy ? i18n.t("db_app.unlocking")
               : (errorText.length > 0 ? errorText : i18n.t("common.footer_confirm_cancel"))
    hintColor: errorText.length > 0 ? theme.alertError : theme.guidance

    capturesKeys: false

    onShown: {
        passwordField.text = "";
        passwordField.field.forceActiveFocus();
    }

    // A failed attempt clears the box and puts the cursor back, so the next
    // try starts clean without a trip to the mouse.
    onErrorTextChanged: {
        if (errorText.length > 0) {
            passwordField.text = "";
            passwordField.field.forceActiveFocus();
        }
    }

    Field {
        id: passwordField
        width: parent.width
        label: i18n.t("common.password_label")
        echoMode: TextInput.Password
        enabled: !root.busy

        onSubmitted: root.submitted(text)
        onCancelled: root.dismissed()
    }
}
