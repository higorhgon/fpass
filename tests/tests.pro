QT += core testlib
CONFIG += testcase c++17
TEMPLATE = app
TARGET = tst_omapass

INCLUDEPATH += ../src

SOURCES += \
    tst_omapass.cpp \
    ../src/config.cpp \
    ../src/filter.cpp \
    ../src/history.cpp \
    ../src/i18n.cpp \
    ../src/kdbx2pass.cpp \
    ../src/passstore.cpp \
    ../src/process.cpp \
    ../src/secret.cpp

HEADERS += \
    ../src/config.h \
    ../src/filter.h \
    ../src/history.h \
    ../src/i18n.h \
    ../src/kdbx2pass.h \
    ../src/passstore.h \
    ../src/process.h \
    ../src/secret.h

RESOURCES += ../src/resources.qrc
