import QtQuick
import QtQuick.Controls
import QtQuick.Window

ApplicationWindow {
    id: window

    width: 760
    height: 560
    minimumWidth: 460
    minimumHeight: 340
    visible: true
    title: "omapass"

    // Every size in the interface is written against the desktop's text
    // scale, so the whole face grows with `omarchy display text size` the way
    // the rest of the family does.
    readonly property real s: systemTheme.textScale

    // Set by whichever page currently has a text field focused, so Ctrl+C
    // keeps its clipboard meaning while typing and only quits otherwise.
    property bool editingText: false

    color: theme.background

    Shortcut {
        sequence: "Ctrl+Q"
        context: Qt.ApplicationShortcut
        onActivated: window.close()
    }

    Shortcut {
        sequence: "Ctrl+C"
        context: Qt.ApplicationShortcut
        enabled: !window.editingText
        onActivated: window.close()
    }

    Loader {
        id: pageLoader
        anchors.fill: parent
        sourceComponent: controller.stage === "databases" ? databasePage : entriesPage
        // No forceActiveFocus() here: the page's own list pane claims the
        // keyboard when it should have it, and forcing focus onto the page
        // after loading would take it straight back off a sheet that opened
        // during construction.
        focus: true
    }

    Component {
        id: databasePage
        DatabasePage {}
    }

    Component {
        id: entriesPage
        EntriesPage {}
    }

    // Remember the last windowed geometry rather than whatever the window
    // happens to measure at teardown: a maximized window reports screen-sized
    // dimensions, and the close sequence hides the window before destruction,
    // so neither the live geometry nor the final visibility can be trusted.
    property rect normalGeometry: Qt.rect(x, y, width, height)
    property bool wasMaximized: false

    function trackNormalGeometry() {
        if (visibility === Window.Windowed)
            normalGeometry = Qt.rect(x, y, width, height);
    }

    onXChanged: trackNormalGeometry()
    onYChanged: trackNormalGeometry()
    onWidthChanged: trackNormalGeometry()
    onHeightChanged: trackNormalGeometry()

    onVisibilityChanged: function(mode) {
        if (mode === Window.Maximized || mode === Window.FullScreen)
            wasMaximized = true;
        else if (mode === Window.Windowed)
            wasMaximized = false;
    }

    Component.onCompleted: {
        var geometry = controller.windowGeometry();
        if (geometry.valid) {
            x = geometry.x;
            y = geometry.y;
            width = geometry.width;
            height = geometry.height;
            if (geometry.maximized)
                showMaximized();
        } else {
            width = Math.round(760 * systemTheme.textScale);
            height = Math.round(560 * systemTheme.textScale);
        }
    }

    Component.onDestruction: controller.saveWindowGeometry(
        normalGeometry.x, normalGeometry.y,
        normalGeometry.width, normalGeometry.height, wasMaximized)
}
