#pragma once

#include "input/ControllerInput.h"
#include "opennow_streamer_ffi.h"

inline OpenNowSonySnapshot openNowWireSonySnapshot(const ControllerInput::SonySnapshot &snapshot)
{
    OpenNowSonySnapshot wire{};
    wire.version = OPENNOW_STREAMER_SONY_SNAPSHOT_VERSION;
    wire.struct_size = sizeof(OpenNowSonySnapshot);
    wire.slot = snapshot.slot;
    wire.touchpad_click = snapshot.touchpadClick ? 1u : 0u;
    wire.incarnation = snapshot.incarnation;
    wire.buttons = snapshot.buttons;
    wire.left_trigger = snapshot.leftTrigger;
    wire.right_trigger = snapshot.rightTrigger;
    wire.left_stick_x = snapshot.leftStickX;
    wire.left_stick_y = snapshot.leftStickY;
    wire.right_stick_x = snapshot.rightStickX;
    wire.right_stick_y = snapshot.rightStickY;
    wire.contact_active[0] = snapshot.contacts[0].active ? 1u : 0u;
    wire.contact_active[1] = snapshot.contacts[1].active ? 1u : 0u;
    wire.contact_x[0] = snapshot.contacts[0].x;
    wire.contact_y[0] = snapshot.contacts[0].y;
    wire.contact_x[1] = snapshot.contacts[1].x;
    wire.contact_y[1] = snapshot.contacts[1].y;
    wire.observed_at_us = snapshot.observedAtUs;
    return wire;
}
