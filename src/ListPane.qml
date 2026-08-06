import QtQuick
import QtQuick.Controls

// The search line, the list and the hint footer — the shape both stages of
// omapass share — together with the whole keyboard vocabulary. The pages above
// decide what each gesture means; nothing here knows about vaults.
FocusScope {
    id: pane

    property string searchPlaceholder: ""
    property var model: []
    property string footerText: ""
    property color footerColor: theme.guidance
    property bool searchMode: true
    property bool acceptsKeys: true
    // Bracketed tag drawn before a row's text, or "" for none.
    property var tagFor: function(item) { return ""; }

    property alias query: searchInput.text
    property alias currentIndex: list.currentIndex
    readonly property bool typing: searchInput.activeFocus

    signal activated(int index)
    signal detailsRequested()
    signal menuRequested(real menuX, real menuY)
    signal addRequested()
    signal editRequested()
    signal deleteRequested()
    signal helpRequested()
    signal quitRequested()

    readonly property real s: systemTheme.textScale
    readonly property int rowHeight: Math.round(30 * s)

    property bool lastKeyWasG: false

    function next() {
        if (list.count === 0)
            return;
        list.currentIndex = list.currentIndex >= list.count - 1 ? 0 : list.currentIndex + 1;
    }

    function previous() {
        if (list.count === 0)
            return;
        list.currentIndex = list.currentIndex <= 0 ? list.count - 1 : list.currentIndex - 1;
    }

    function goToTop() {
        if (list.count > 0)
            list.currentIndex = 0;
    }

    function goToBottom() {
        if (list.count > 0)
            list.currentIndex = list.count - 1;
    }

    function halfPage() {
        return Math.max(1, Math.floor(list.height / pane.rowHeight / 2));
    }

    function halfPageDown() {
        if (list.count > 0)
            list.currentIndex = Math.min(list.count - 1, list.currentIndex + halfPage());
    }

    function halfPageUp() {
        if (list.count > 0)
            list.currentIndex = Math.max(0, list.currentIndex - halfPage());
    }

    function enterSearch() {
        pane.searchMode = true;
        searchInput.forceActiveFocus();
    }

    function leaveSearch() {
        pane.searchMode = false;
        keyCatcher.forceActiveFocus();
    }

    // Chords that work the same whether or not the search line has focus.
    function handleControlKey(event) {
        if (!(event.modifiers & Qt.ControlModifier))
            return false;

        switch (event.key) {
        case Qt.Key_D: halfPageDown(); return true;
        case Qt.Key_U: halfPageUp(); return true;
        case Qt.Key_N: next(); return true;
        case Qt.Key_P: previous(); return true;
        case Qt.Key_A: pane.addRequested(); return true;
        case Qt.Key_E: pane.editRequested(); return true;
        case Qt.Key_X: pane.deleteRequested(); return true;
        // Ctrl+? reaches applications as Ctrl+Shift+/ or plain Ctrl+/
        // depending on the layout; both open the shortcut sheet.
        case Qt.Key_Question:
        case Qt.Key_Slash:
            pane.helpRequested();
            return true;
        }
        return false;
    }

    // Anchored rather than laid out in a Column: the list has to take the
    // space the search line and the footer leave over, which a column cannot
    // express without the list's height depending on its own position.
    Item {
        id: content
        anchors.fill: parent
        anchors.margins: Math.round(24 * pane.s)

        Item {
            id: searchRow
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            height: Math.round(38 * pane.s)

            TextField {
                id: searchInput
                anchors.fill: parent
                placeholderText: pane.searchPlaceholder
                placeholderTextColor: theme.guidance
                color: theme.base
                font.pixelSize: Math.round(17 * pane.s)
                selectionColor: theme.selection
                selectedTextColor: theme.base
                leftPadding: 0
                rightPadding: 0
                background: Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: Math.max(1, Math.round(pane.s))
                    color: pane.searchMode ? theme.annotation : theme.guidance
                    opacity: pane.searchMode ? 1.0 : 0.4
                }

                Keys.onPressed: function(event) {
                    if (!pane.acceptsKeys)
                        return;
                    if (pane.handleControlKey(event)) {
                        event.accepted = true;
                        return;
                    }
                    if (event.modifiers & Qt.ControlModifier)
                        return;

                    switch (event.key) {
                    case Qt.Key_Escape:
                        pane.leaveSearch();
                        break;
                    case Qt.Key_Down:
                        pane.next();
                        break;
                    case Qt.Key_Up:
                        pane.previous();
                        break;
                    case Qt.Key_Return:
                    case Qt.Key_Enter:
                        pane.activated(list.currentIndex);
                        break;
                    case Qt.Key_Tab:
                        pane.detailsRequested();
                        break;
                    default:
                        return;
                    }
                    event.accepted = true;
                }
            }
        }

        Item {
            anchors.top: searchRow.bottom
            anchors.bottom: footer.top
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.topMargin: Math.round(14 * pane.s)
            anchors.bottomMargin: Math.round(14 * pane.s)

            ListView {
                id: list
                anchors.fill: parent
                model: pane.model
                clip: true
                keyNavigationEnabled: false
                highlightMoveDuration: 0
                currentIndex: 0
                boundsBehavior: Flickable.StopAtBounds

                // Re-filtering always lands on the first match, as the TUI
                // did on every keystroke. Deferred because `count` still
                // reflects the previous model while this fires.
                onModelChanged: Qt.callLater(function() {
                    list.currentIndex = list.count > 0 ? 0 : -1;
                })

                ScrollBar.vertical: ScrollBar {}

                delegate: Rectangle {
                    id: row
                    width: list.width
                    height: pane.rowHeight
                    radius: Math.round(4 * pane.s)
                    color: list.currentIndex === index ? theme.selection
                         : hover.hovered ? Qt.rgba(theme.selection.r, theme.selection.g,
                                                   theme.selection.b, 0.4)
                         : "transparent"

                    readonly property string itemText: typeof modelData === "string"
                        ? modelData : (modelData.path !== undefined ? modelData.path : "")
                    readonly property string tag: pane.tagFor(modelData)

                    HoverHandler {
                        id: hover
                    }

                    Row {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.leftMargin: Math.round(10 * pane.s)
                        anchors.rightMargin: Math.round(10 * pane.s)
                        spacing: Math.round(8 * pane.s)

                        Text {
                            visible: row.tag.length > 0
                            text: row.tag
                            color: theme.annotation
                            font.pixelSize: Math.round(15 * pane.s)
                            anchors.verticalCenter: parent.verticalCenter
                        }

                        Text {
                            width: parent.width - (row.tag.length > 0 ? x : 0)
                            text: row.itemText
                            color: theme.base
                            elide: Text.ElideMiddle
                            font.pixelSize: Math.round(15 * pane.s)
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }

                    TapHandler {
                        acceptedButtons: Qt.LeftButton
                        onSingleTapped: {
                            list.currentIndex = index;
                            pane.leaveSearch();
                        }
                        onDoubleTapped: {
                            list.currentIndex = index;
                            pane.activated(index);
                        }
                    }

                    TapHandler {
                        acceptedButtons: Qt.RightButton
                        onSingleTapped: function(eventPoint) {
                            list.currentIndex = index;
                            const scenePoint = row.mapToItem(pane, eventPoint.position.x,
                                                             eventPoint.position.y);
                            pane.menuRequested(scenePoint.x, scenePoint.y);
                        }
                    }
                }
            }
        }

        Text {
            id: footer
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            text: pane.footerText
            color: pane.footerColor
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            font.pixelSize: Math.round(13 * pane.s)
        }
    }

    // Holds focus whenever the search line does not, so the vim-style keys
    // reach the pane instead of being typed into the query.
    Item {
        id: keyCatcher
        anchors.fill: parent
        focus: !pane.searchMode
        z: -1

        Keys.onPressed: function(event) {
            if (!pane.acceptsKeys)
                return;
            if (pane.handleControlKey(event)) {
                event.accepted = true;
                pane.lastKeyWasG = false;
                return;
            }
            if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))
                return;

            var wasG = false;
            switch (event.key) {
            case Qt.Key_Q:
            case Qt.Key_Escape:
                pane.quitRequested();
                break;
            case Qt.Key_J:
            case Qt.Key_Down:
                pane.next();
                break;
            case Qt.Key_K:
            case Qt.Key_Up:
                pane.previous();
                break;
            case Qt.Key_Return:
            case Qt.Key_Enter:
                pane.activated(list.currentIndex);
                break;
            case Qt.Key_Tab:
                pane.detailsRequested();
                break;
            case Qt.Key_Space:
                pane.menuRequested(-1, -1);
                break;
            case Qt.Key_Slash:
            case Qt.Key_F:
            case Qt.Key_I:
                pane.enterSearch();
                break;
            case Qt.Key_G:
                if (event.modifiers & Qt.ShiftModifier) {
                    pane.goToBottom();
                } else if (pane.lastKeyWasG) {
                    pane.goToTop();
                } else {
                    wasG = true;
                }
                break;
            default:
                return;
            }

            pane.lastKeyWasG = wasG;
            event.accepted = true;
        }
    }

    // The pane only ever takes the keyboard while the page says it may: a
    // sheet on top owns it otherwise, and grabbing focus regardless would
    // send typing to the search line behind the sheet.
    function claimKeyboard() {
        if (!pane.acceptsKeys)
            return;
        if (pane.searchMode)
            searchInput.forceActiveFocus();
        else
            keyCatcher.forceActiveFocus();
    }

    onAcceptsKeysChanged: claimKeyboard()

    Component.onCompleted: claimKeyboard()
}
