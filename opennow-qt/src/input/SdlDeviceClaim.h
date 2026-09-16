#pragma once

#include <QtGlobal>

inline constexpr int maxSdlSources = 4;

struct SdlDeviceClaim {
    quint8 slot = 0;
    quint64 incarnation = 0;
    quint16 vendor = 0;
    quint16 product = 0;
};
