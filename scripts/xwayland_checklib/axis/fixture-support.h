/* SPDX-License-Identifier: MIT */
#ifndef AXIS_CALLBACK_FIXTURE_SUPPORT_H
#define AXIS_CALLBACK_FIXTURE_SUPPORT_H

#include <xwayland-config.h>
#include <inputstr.h>
#include <inpututils.h>
#include "xwayland-input.h"
#include "xwayland-input-target.h"
#include "xwayland-screen.h"

struct fixture_state {
    struct xwl_seat seat;
    struct xwl_screen screen;
    struct xwl_window focus;
    ScreenRec xscreen;
    DeviceIntRec device;
    unsigned pointer_token;
    unsigned version;
    unsigned frame;
    unsigned events;
    unsigned drains;
    const char *name;
};

extern struct fixture_state axis_fixture;
void fixture_fail(const char *message) _X_NORETURN;
void fixture_begin(const char *name, unsigned version, Bool enabled);
void fixture_end(void);

#endif
