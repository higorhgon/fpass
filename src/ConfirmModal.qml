import QtQuick

// A yes/no question, answered with y or n exactly as in the TUI.
Modal {
    id: root

    property string question: ""

    signal accepted()

    accentColor: theme.important
    heading: i18n.t("common.confirm_title")
    hint: i18n.t("common.yes_no_hint")
    cardWidth: Math.round(460 * s)

    onKeyPressed: function(event) {
        switch (event.key) {
        case Qt.Key_Y:
            root.accepted();
            break;
        case Qt.Key_N:
        case Qt.Key_Escape:
        case Qt.Key_Return:
        case Qt.Key_Enter:
            root.dismissed();
            break;
        default:
            return;
        }
        event.accepted = true;
    }

    Text {
        width: parent.width
        text: root.question
        color: theme.base
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
        font.family: "iA Writer Mono S"
        font.pixelSize: Math.round(14 * root.s)
    }
}
