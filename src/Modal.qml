import QtQuick
import QtQuick.Controls

// Shared frame for every sheet fpass puts over the list: a bordered card,
// a heading in the accent colour, and a hint line at the bottom — the same
// three parts the boxed modals of the TUI had.
//
// Visibility stays owned by the page (bound to its current mode), so this
// never closes itself: clicking the dimmed backdrop and pressing keys are
// reported as signals for the page to act on.
Popup {
    id: root

    property string heading: ""
    property color accentColor: theme.alertInfo
    property string hint: ""
    property color hintColor: theme.guidance
    property real cardWidth: Math.round(520 * root.s)

    // Sheets made only of buttons and text rely on this; ones with fields
    // turn it off and let the focused field handle its own keys, otherwise
    // the catcher below takes the focus the field needs to be typed into.
    property bool capturesKeys: true
    signal keyPressed(var event)
    signal dismissed()

    // Popup only emits opened() after its enter transition, and a sheet whose
    // visibility is bound to the page's mode can be shown before that ever
    // runs. `shown` fires once per appearance whichever path got us here, so
    // sheets have a dependable moment to reset fields and take the keyboard.
    signal shown()

    property bool announcedShown: false

    onVisibleChanged: {
        if (!visible)
            announcedShown = false;
        else if (!announcedShown)
            announceShown();
    }

    onOpened: if (!announcedShown) announceShown()

    function announceShown() {
        announcedShown = true;
        shown();
    }

    default property alias body: bodyColumn.data

    readonly property real s: systemTheme.textScale
    readonly property real gap: Math.round(14 * s)

    anchors.centerIn: Overlay.overlay
    width: Math.min(cardWidth, parent ? parent.width - Math.round(40 * s) : cardWidth)
    modal: true
    focus: true
    padding: Math.round(20 * s)
    closePolicy: Popup.NoAutoClose

    // A tall sheet (the full entry form, say) is capped to the window and
    // scrolls its body rather than growing past the edges and losing its
    // heading and hint off-screen.
    implicitHeight: topPadding + bottomPadding + headingText.implicitHeight
        + hintText.implicitHeight + bodyColumn.implicitHeight + root.gap * 2
    height: Math.min(implicitHeight,
                     parent ? parent.height - Math.round(40 * s) : implicitHeight)

    Overlay.modal: Rectangle {
        color: Qt.rgba(0, 0, 0, 0.45)

        TapHandler {
            onTapped: root.dismissed()
        }
    }

    background: Rectangle {
        color: theme.background
        radius: Math.round(8 * root.s)
        border.width: Math.max(1, Math.round(root.s))
        border.color: root.accentColor
    }

    contentItem: Item {
        Item {
            id: keyCatcher
            width: 0
            height: 0
            focus: root.capturesKeys

            Keys.onPressed: function(event) {
                root.keyPressed(event);
            }
        }

        Text {
            id: headingText
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            visible: root.heading.length > 0
            text: root.heading
            color: root.accentColor
            elide: Text.ElideMiddle
            font.family: "iA Writer Mono S"
            font.bold: true
            font.pixelSize: Math.round(15 * root.s)
        }

        ScrollView {
            id: bodyArea
            anchors.top: headingText.bottom
            anchors.bottom: hintText.top
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.topMargin: root.gap
            anchors.bottomMargin: root.gap
            contentWidth: availableWidth

            Column {
                id: bodyColumn
                width: bodyArea.availableWidth
                spacing: Math.round(12 * root.s)
            }
        }

        Text {
            id: hintText
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            visible: root.hint.length > 0
            text: root.hint
            color: root.hintColor
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            font.family: "iA Writer Mono S"
            font.pixelSize: Math.round(12 * root.s)
        }
    }
}
