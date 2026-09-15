#include "media/ThumbnailGenerator.h"

#include <QDir>
#include <QFile>
#include <QScopeGuard>
#include <QSignalSpy>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QtTest>

class ThumbnailGeneratorTest final : public QObject
{
    Q_OBJECT

private slots:
    void rejectsUntrustedAndNonVideoPaths();
    void regenerationScopingFollowsPicturesOverride();
    void regenerationRefusesUnavailablePicturesRoot();
};

void ThumbnailGeneratorTest::rejectsUntrustedAndNonVideoPaths()
{
    ThumbnailGenerator generator;
    QVERIFY(!generator.regenerate(QStringLiteral("/tmp/outside.mkv")));
    const auto previousPictures = qgetenv("OPENNOW_PICTURES_DIR");
    const auto restoreEnvironment = qScopeGuard([&] {
        if (previousPictures.isNull()) qunsetenv("OPENNOW_PICTURES_DIR");
        else qputenv("OPENNOW_PICTURES_DIR", previousPictures);
    });
    qunsetenv("OPENNOW_PICTURES_DIR");
    const auto pictures = QStandardPaths::writableLocation(QStandardPaths::PicturesLocation);
    QDir directory(pictures);
    QVERIFY(directory.mkpath(QStringLiteral("OpenNOW/Recordings")));
    const auto invalid = directory.filePath(QStringLiteral("OpenNOW/Recordings/not-video.txt"));
    QFile file(invalid);
    QVERIFY(file.open(QIODevice::WriteOnly));
    QCOMPARE(file.write("fixture"), 7);
    file.close();
    QVERIFY(!generator.regenerate(invalid));
    QVERIFY(!generator.busy());
    QFile::remove(invalid);
}

void ThumbnailGeneratorTest::regenerationScopingFollowsPicturesOverride()
{
    QTemporaryDir overrideRoot;
    QVERIFY(overrideRoot.isValid());
    const auto previousPictures = qgetenv("OPENNOW_PICTURES_DIR");
    const auto restoreEnvironment = qScopeGuard([&] {
        if (previousPictures.isNull()) qunsetenv("OPENNOW_PICTURES_DIR");
        else qputenv("OPENNOW_PICTURES_DIR", previousPictures);
    });
    qputenv("OPENNOW_PICTURES_DIR", overrideRoot.path().toUtf8());

    QDir overrideRecordings(
        QDir(overrideRoot.path()).filePath(QStringLiteral("OpenNOW/Recordings")));
    QVERIFY(overrideRecordings.mkpath(QStringLiteral(".")));
    const auto insideOverride = overrideRecordings.filePath(QStringLiteral("override-clip.mkv"));
    QFile inside(insideOverride);
    QVERIFY(inside.open(QIODevice::WriteOnly));
    QCOMPARE(inside.write("fixture"), 7);
    inside.close();

    QDir defaultRecordings(QDir(QStandardPaths::writableLocation(QStandardPaths::PicturesLocation))
                               .filePath(QStringLiteral("OpenNOW/Recordings")));
    QVERIFY(defaultRecordings.mkpath(QStringLiteral(".")));
    const auto outsideOverride = defaultRecordings.filePath(QStringLiteral("default-clip.mkv"));
    QFile outside(outsideOverride);
    QVERIFY(outside.open(QIODevice::WriteOnly));
    QCOMPARE(outside.write("fixture"), 7);
    outside.close();

    ThumbnailGenerator generator;
    QVERIFY(!generator.regenerate(outsideOverride));
    QVERIFY(generator.regenerate(insideOverride));
    QVERIFY(generator.busy());
    QFile::remove(outsideOverride);
}

void ThumbnailGeneratorTest::regenerationRefusesUnavailablePicturesRoot()
{
    const auto previousPictures = qgetenv("OPENNOW_PICTURES_DIR");
    const auto restoreEnvironment = qScopeGuard([&] {
        if (previousPictures.isNull()) qunsetenv("OPENNOW_PICTURES_DIR");
        else qputenv("OPENNOW_PICTURES_DIR", previousPictures);
    });
    qputenv("OPENNOW_PICTURES_DIR", "");

    QDir recordings(QDir(QStandardPaths::writableLocation(QStandardPaths::PicturesLocation))
                        .filePath(QStringLiteral("OpenNOW/Recordings")));
    QVERIFY(recordings.mkpath(QStringLiteral(".")));
    const auto source = recordings.filePath(QStringLiteral("unavailable-clip.mkv"));
    QFile file(source);
    QVERIFY(file.open(QIODevice::WriteOnly));
    QCOMPARE(file.write("fixture"), 7);
    file.close();

    ThumbnailGenerator generator;
    QVERIFY(!generator.regenerate(source));
    QVERIFY(!generator.busy());
    QFile::remove(source);
}

QTEST_MAIN(ThumbnailGeneratorTest)
#include "tst_thumbnailgenerator.moc"
