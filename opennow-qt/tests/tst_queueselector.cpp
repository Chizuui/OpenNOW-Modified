#include <QFontDatabase>
#include <QDesktopServices>
#include <QFont>
#include <QGuiApplication>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQmlPropertyMap>
#include <QtQuickTest/quicktest.h>

class QueueSelectorTestSetup final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QUrl openedUrl READ openedUrl NOTIFY openedUrlChanged)

public:
    QUrl openedUrl() const { return m_openedUrl; }

signals:
    void openedUrlChanged();

public slots:
    void captureUrl(const QUrl &url)
    {
        m_openedUrl = url;
        emit openedUrlChanged();
    }

    void applicationAvailable()
    {
        qputenv("QT_QUICK_CONTROLS_STYLE", "Basic");
        QDesktopServices::setUrlHandler("https", this, "captureUrl");
        const auto source = QStringLiteral(OPENNOW_QML_SOURCE_DIR);
        qmlRegisterSingletonType(QUrl::fromLocalFile(source + "/theme/Theme.qml"), "OpenNOW", 1, 0, "Theme");
        qmlRegisterSingletonType(QUrl::fromLocalFile(source + "/desktop/components/DesktopTokens.qml"), "OpenNOW", 1, 0, "DesktopTokens");
        qmlRegisterSingletonType(QUrl::fromLocalFile(source + "/components/InputPromptIcons.qml"), "OpenNOW", 1, 0, "InputPromptIcons");
        qmlRegisterType(QUrl::fromLocalFile(source + "/state/account/QueueSelectorState.qml"), "OpenNOW", 1, 0, "QueueSelectorState");
        qmlRegisterType(QUrl::fromLocalFile(source + "/desktop/components/DesktopQueueSelector.qml"), "OpenNOW", 1, 0, "DesktopQueueSelector");
        qmlRegisterType(QUrl::fromLocalFile(source + "/components/KeyboardGlyph.qml"), "OpenNOW", 1, 0, "KeyboardGlyph");
        for (const auto *name : {"DesktopSettingsButton", "DesktopSettingsIcon"}) {
            qmlRegisterType(QUrl::fromLocalFile(source + "/desktop/settings/controls/" + name + ".qml"), "OpenNOW", 1, 0, name);
        }
        for (const auto *font : {"Nunito-Variable.ttf", "IBMPlexMono-Regular.ttf",
                 "IBMPlexMono-Medium.ttf", "IBMPlexMono-Bold.ttf"}) {
            QFontDatabase::addApplicationFont(source + "/../res/fonts/" + font);
        }
        QFont applicationFont(QStringLiteral("Nunito"));
        applicationFont.setHintingPreference(QFont::PreferNoHinting);
        applicationFont.setStyleStrategy(QFont::PreferAntialias);
        QGuiApplication::setFont(applicationFont);
    }

    void qmlEngineAvailable(QQmlEngine *engine)
    {
        auto paths = engine->importPathList();
        paths.removeAll(QCoreApplication::applicationDirPath());
        engine->setImportPathList(paths);
        m_shell.insert("settings", QVariantMap{{"appTheme", "dark"}});
        m_shell.insert("previewThemePack", QString{});
        m_controller.insert("reducedMotion", true);
        engine->rootContext()->setContextProperty("ShellStore", &m_shell);
        engine->rootContext()->setContextProperty("AppController", &m_controller);
        engine->rootContext()->setContextProperty("UrlCapture", this);
    }

private:
    QUrl m_openedUrl;
    QQmlPropertyMap m_shell;
    QQmlPropertyMap m_controller;
};

QUICK_TEST_MAIN_WITH_SETUP(queueselector, QueueSelectorTestSetup)
#include "tst_queueselector.moc"
