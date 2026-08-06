import QtQuick

// First stage: pick a database, then unlock it. Also where new KeePassXC
// databases and pass stores are created.
FocusScope {
    id: page

    property string mode: controller.anyDatabaseFound ? "list" : "confirmCreate"

    readonly property bool unlocking: Object.keys(controller.pendingDatabase).length > 0

    // Ctrl+C keeps its clipboard meaning wherever text is being typed, and
    // only quits from the plain list.
    Binding {
        target: window
        property: "editingText"
        value: page.mode !== "list" || pane.typing
    }

    ListPane {
        id: pane
        anchors.fill: parent
        focus: !page.unlocking && page.mode === "list"
        acceptsKeys: !page.unlocking && page.mode === "list"

        searchPlaceholder: i18n.t("db_app.filter_title")
        model: controller.databases
        tagFor: function(item) { return "[" + item.kind + "]"; }
        onQueryChanged: controller.databaseQuery = query

        footerText: controller.hasMessage ? controller.message
                  : page.mode === "list" ? (searchMode ? i18n.t("db_app.footer_search")
                                                       : i18n.t("db_app.footer_normal"))
                  : ""
        footerColor: controller.hasMessage
            ? (controller.messageIsError ? theme.alertError : theme.alertInfo)
            : theme.guidance

        onActivated: function(index) { controller.selectDatabase(index); }
        onAddRequested: page.mode = "chooseType"
        onHelpRequested: page.mode = "help"
        onQuitRequested: Qt.quit()
    }

    ConfirmModal {
        visible: page.mode === "confirmCreate"
        accentColor: theme.alertWarn
        question: i18n.t("db_app.no_db_found_confirm")

        onAccepted: page.mode = "chooseType"
        onDismissed: Qt.quit()
    }

    ChoiceModal {
        visible: page.mode === "chooseType"
        heading: i18n.t("db_app.new_db_title")
        hint: i18n.t("db_app.footer_choose_type")
        cardWidth: Math.round(360 * window.s)
        options: ["KeePassXC (.kdbx)", "pass"]

        onChosen: function(index) {
            page.mode = index === 0 ? "createDb" : "createPass";
        }
        onDismissed: page.mode = controller.anyDatabaseFound ? "list" : "confirmCreate"
    }

    CreateDatabaseModal {
        visible: page.mode === "createDb"
        busy: controller.busy
        errorText: controller.unlockError

        onSubmitted: function(name, password) { controller.createKeepassDatabase(name, password); }
        onDismissed: page.mode = "chooseType"
    }

    CreatePassStoreModal {
        visible: page.mode === "createPass"
        busy: controller.busy
        errorText: controller.unlockError

        onSubmitted: function(directory, keyId) { controller.createPassStore(directory, keyId); }
        onDismissed: page.mode = "chooseType"
    }

    UnlockModal {
        visible: page.unlocking
        databaseName: controller.pendingDatabase.name || ""
        errorText: controller.unlockError
        busy: controller.busy

        onSubmitted: function(password) { controller.unlock(password); }
        onDismissed: {
            controller.cancelUnlock();
            page.mode = "list";
        }
    }

    HelpModal {
        visible: page.mode === "help"
        sections: [
            {
                "title": i18n.t("help.nav_section"),
                "items": [["j/k, ↑/↓", i18n.t("help.move_selection")],
                          ["gg / G", i18n.t("help.go_top_bottom")],
                          ["CTRL-U/D", i18n.t("help.half_page")],
                          ["CTRL-N/P", i18n.t("help.next_prev")]]
            },
            {
                "title": i18n.t("help.actions_section"),
                "items": [["ENTER", i18n.t("db_app.help_select_db")],
                          ["CTRL-A", i18n.t("db_app.help_create_new_db")]]
            },
            {
                "title": i18n.t("help.search_section"),
                "items": [["/, f, i", i18n.t("help.enter_search")],
                          ["ESC", i18n.t("db_app.help_exit_search")]]
            },
            {
                "title": i18n.t("help.general_section"),
                "items": [["CTRL+?", i18n.t("help.this_help")],
                          ["CTRL-C, CTRL-Q", i18n.t("help.quit_fpass")]]
            }
        ]

        onDismissed: page.mode = "list"
    }

    // A database created here opens straight away, so the creation sheet
    // steps aside as soon as it succeeds.
    Connections {
        target: controller

        function onDatabaseCreated() {
            page.mode = "list";
        }
    }
}
