/* SPDX-License-Identifier: MIT */
#ifndef FIXTURE_INPUT_SOURCE
#error "The runner must select an explicit candidate xwayland-input.c"
#endif
#ifndef FIXTURE_TARGET_SOURCE
#error "The runner must select that candidate's xwayland-input-target.c"
#endif

/* Entire, unmodified candidate translation units; direct calls below reach
 * their actual static callbacks. No callback body is copied or substituted. */
#include FIXTURE_INPUT_SOURCE
#include FIXTURE_TARGET_SOURCE
#include "fixture-support.h"
#include <string.h>

static void
finish_frame(void)
{
    pointer_handle_frame(&axis_fixture.seat, axis_fixture.seat.wl_pointer);
    ++axis_fixture.frame;
}

static void
normal_button(uint32_t state)
{
    pointer_handle_button(&axis_fixture.seat, axis_fixture.seat.wl_pointer,
                          101, 1234, BTN_LEFT, state);
}

static void
continuous(uint32_t axis, int value)
{
    pointer_handle_axis(&axis_fixture.seat, axis_fixture.seat.wl_pointer,
                        1234, axis, wl_fixed_from_int(value));
}

static void
discrete(uint32_t axis, int value)
{
    pointer_handle_axis_discrete(&axis_fixture.seat,
                                 axis_fixture.seat.wl_pointer, axis, value);
}

static void
v120(uint32_t axis, int value)
{
    pointer_handle_axis_v120(&axis_fixture.seat,
                             axis_fixture.seat.wl_pointer, axis, value);
}

static void
split_pair(uint32_t axis, Bool high_resolution)
{
    if (high_resolution)
        v120(axis, -120);
    else
        discrete(axis, -1);
    normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
    normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
    continuous(axis, -10);
    finish_frame();
    /* Continuous-only input in the next frame must not inherit metadata. */
    continuous(axis, -5);
    finish_frame();
}

static void
run_case(unsigned which)
{
    switch (which) {
    case 0:
        split_pair(WL_POINTER_AXIS_VERTICAL_SCROLL, FALSE);
        break;
    case 1:
        split_pair(WL_POINTER_AXIS_HORIZONTAL_SCROLL, FALSE);
        break;
    case 2:
        discrete(WL_POINTER_AXIS_VERTICAL_SCROLL, -1);
        discrete(WL_POINTER_AXIS_HORIZONTAL_SCROLL, 1);
        normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
        normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
        continuous(WL_POINTER_AXIS_VERTICAL_SCROLL, -10);
        continuous(WL_POINTER_AXIS_HORIZONTAL_SCROLL, 10);
        finish_frame();
        break;
    case 3:
        discrete(WL_POINTER_AXIS_VERTICAL_SCROLL, -1);
        continuous(WL_POINTER_AXIS_VERTICAL_SCROLL, -10);
        discrete(WL_POINTER_AXIS_HORIZONTAL_SCROLL, 2);
        continuous(WL_POINTER_AXIS_HORIZONTAL_SCROLL, 20);
        finish_frame();
        finish_frame();
        break;
    case 4:
        continuous(WL_POINTER_AXIS_VERTICAL_SCROLL, -5);
        continuous(WL_POINTER_AXIS_HORIZONTAL_SCROLL, 20);
        finish_frame();
        continuous(WL_POINTER_AXIS_VERTICAL_SCROLL, -10);
        finish_frame();
        break;
    case 5:
        continuous(WL_POINTER_AXIS_VERTICAL_SCROLL, -5);
        normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
        normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
        continuous(WL_POINTER_AXIS_HORIZONTAL_SCROLL, 20);
        finish_frame();
        break;
    case 6:
        v120(WL_POINTER_AXIS_VERTICAL_SCROLL, -120);
        v120(WL_POINTER_AXIS_HORIZONTAL_SCROLL, 60);
        finish_frame();
        continuous(WL_POINTER_AXIS_VERTICAL_SCROLL, -10);
        continuous(WL_POINTER_AXIS_HORIZONTAL_SCROLL, 20);
        finish_frame();
        v120(WL_POINTER_AXIS_VERTICAL_SCROLL, -240);
        finish_frame();
        finish_frame();
        v120(WL_POINTER_AXIS_HORIZONTAL_SCROLL, 30);
        finish_frame();
        break;
    case 7:
        split_pair(WL_POINTER_AXIS_VERTICAL_SCROLL, TRUE);
        break;
    case 8:
        split_pair(WL_POINTER_AXIS_HORIZONTAL_SCROLL, TRUE);
        break;
    default:
        fixture_fail("unknown case");
    }
}

#include "fixture-extended.h"

static void
run_initial_cases(void)
{
    static const char *const names[] = {
        "discrete_vertical_split", "discrete_horizontal_split",
        "discrete_dual_split", "discrete_adjacent", "continuous",
        "continuous_split", "v120_standalone_reset",
        "v120_vertical_split", "v120_horizontal_split"
    };
    unsigned gate, which, version;

    for (gate = 0; gate <= 1; ++gate) {
        for (which = 0; which < sizeof(names) / sizeof(names[0]); ++which) {
            for (version = which < 6 ? 5 : 8;
                 version <= (which < 6 ? 7 : 8); ++version) {
                fixture_begin(names[which], version, gate);
                run_case(which);
                fixture_end();
            }
        }
    }
}

int
main(int argc, char **argv)
{
    /* A watchdog is only a resource bound; callback inputs/assertions use
     * fixed values and have no timing, sleeps, display, or native dependency. */
    alarm(3);
    if (argc == 1)
        run_initial_cases();
    else if (argc == 2 && strcmp(argv[1], "extended") == 0)
        run_extended_cases();
    else
        fixture_fail("expected no argument or extended");
    if (ferror(stdout))
        fixture_fail("stdout write failed");
    return 0;
}
