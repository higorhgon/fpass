import QtQuick

// Second stage: the entries of the open vault, with everything that can be
// done to them.
FocusScope {
    id: page

    property string mode: "list"
    property string selectedEntry: ""

    readonly property bool menuOpen: actionsMenu.opened

    Binding {
        target: window
        property: "editingText"
        value: page.mode !== "list" || pane.typing
    }

    function currentEntry() {
        const entries = controller.entries;
        if (pane.currentIndex < 0 || pane.currentIndex >= entries.length)
            return "";
        return entries[pane.currentIndex];
    }

    function openInfo() {
        const entry = currentEntry();
        if (entry.length === 0)
            return;

        const details = controller.entryDetails(entry);
        if (Object.keys(details).length === 0)
            return;

        page.selectedEntry = entry;
        infoModal.details = details;
        page.mode = "info";
    }

    function openAddForm() {
        formModal.isEdit = false;
        formModal.fields = {};
        page.mode = "form";
    }

    // Editing an ordinary entry opens the form; editing an empty group opens
    // the rename sheet instead.
    function editSelected() {
        const entry = currentEntry();
        if (entry.length === 0) {
            controller.showMessage(i18n.t("app.no_entry_selected"), true);
            return;
        }

        page.selectedEntry = entry;
        if (controller.isEmptyGroup(entry)) {
            const group = controller.groupNameOf(entry);
            renameModal.currentName = group.split("/").pop();
            page.mode = "rename";
        } else {
            formModal.isEdit = true;
            formModal.fields = controller.entryFields(entry);
            page.mode = "form";
        }
    }

    function confirmDelete() {
        const entry = currentEntry();
        if (entry.length === 0) {
            controller.showMessage(i18n.t("app.no_entry_selected"), true);
            return;
        }

        page.selectedEntry = entry;
        page.mode = "confirmDelete";
    }

    ListPane {
        id: pane
        anchors.fill: parent
        focus: page.mode === "list" && !page.menuOpen
        acceptsKeys: page.mode === "list" && !page.menuOpen

        searchPlaceholder: i18n.t("ui.search_title")
        model: controller.entries
        onQueryChanged: controller.query = query

        footerText: controller.hasMessage ? controller.message
                  : page.mode === "list" ? (searchMode ? i18n.t("ui.footer_search")
                                                       : i18n.t("ui.footer_normal"))
                  : ""
        footerColor: controller.hasMessage
            ? (controller.messageIsError ? theme.alertError : theme.alertInfo)
            : theme.guidance

        onActivated: function(index) {
            const entry = page.currentEntry();
            if (entry.length > 0)
                controller.copyPassword(entry);
        }
        onDetailsRequested: page.openInfo()
        onAddRequested: page.openAddForm()
        onEditRequested: page.editSelected()
        onDeleteRequested: page.confirmDelete()
        onHelpRequested: page.mode = "help"
        onQuitRequested: Qt.quit()
        onMenuRequested: function(menuX, menuY) {
            actionsMenu.anchorX = menuX;
            actionsMenu.anchorY = menuY;
            actionsMenu.selected = 0;
            actionsMenu.open();
        }
    }

    ActionsMenu {
        id: actionsMenu
        parent: page
        options: [i18n.t("ui.context_add_new"), i18n.t("ui.context_edit"),
                  i18n.t("ui.context_delete")]

        onChosen: function(index) {
            close();
            if (index === 0)
                page.openAddForm();
            else if (index === 1)
                page.editSelected();
            else
                page.confirmDelete();
        }
        onClosed: pane.claimKeyboard()
    }

    EntryFormModal {
        id: formModal
        visible: page.mode === "form"
        passBackend: controller.passBackend

        onSubmitted: function(payload) {
            controller.saveEntry(payload);
            page.mode = "list";
        }
        onDismissed: page.mode = "list"
    }

    EntryInfoModal {
        id: infoModal
        visible: page.mode === "info"
        passBackend: controller.passBackend

        onDismissed: page.mode = "list"
    }

    RenameGroupModal {
        id: renameModal
        visible: page.mode === "rename"

        onSubmitted: function(name) {
            controller.renameGroup(page.selectedEntry, name);
            page.mode = "list";
        }
        onDismissed: page.mode = "list"
    }

    ConfirmModal {
        visible: page.mode === "confirmDelete"
        question: i18n.t("ui.confirm_delete", { "entry": page.selectedEntry })

        onAccepted: {
            controller.deleteEntry(page.selectedEntry);
            page.mode = "list";
        }
        onDismissed: page.mode = "list"
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
                "items": [["ENTER", i18n.t("ui.help_copy_password")],
                          ["TAB", i18n.t("ui.help_view_details")],
                          [i18n.t("common.space_key"), i18n.t("ui.help_open_menu")],
                          ["CTRL-A", i18n.t("ui.help_add_entry")],
                          ["CTRL-E", i18n.t("ui.help_edit_entry")],
                          ["CTRL-X", i18n.t("ui.help_delete_entry")]]
            },
            {
                "title": i18n.t("help.search_section"),
                "items": [["/, f, i", i18n.t("help.enter_search")],
                          ["ESC", i18n.t("ui.help_exit_search")]]
            },
            {
                "title": i18n.t("ui.help_mouse_section"),
                "items": [[i18n.t("ui.help_click_label"), i18n.t("ui.help_select_entry")],
                          [i18n.t("ui.help_double_click_label"), i18n.t("ui.help_copy_password")],
                          [i18n.t("ui.help_right_click_label"), i18n.t("ui.help_context_menu_desc")]]
            },
            {
                "title": i18n.t("help.general_section"),
                "items": [["CTRL+?", i18n.t("help.this_help")],
                          ["CTRL-C, CTRL-Q", i18n.t("help.quit_app")]]
            }
        ]

        onDismissed: page.mode = "list"
    }

    // Returning from any sheet puts the keyboard back on the list, in
    // whichever mode it was left in.
    onModeChanged: if (mode === "list") pane.claimKeyboard()
}
