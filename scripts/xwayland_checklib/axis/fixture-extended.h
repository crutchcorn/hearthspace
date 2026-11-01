/* SPDX-License-Identifier: MIT */
/* Included after fixture.c's callback adapters, in the actual candidate TU.
 * These are input scripts, not a model of pending/representation state. */
#ifndef AXIS_CALLBACK_FIXTURE_EXTENDED_H
#define AXIS_CALLBACK_FIXTURE_EXTENDED_H

static void
stop_axis(uint32_t axis)
{
    pointer_handle_axis_stop(&axis_fixture.seat, axis_fixture.seat.wl_pointer,
                             1234, axis);
}

static void
extended_single_axis(unsigned which, uint32_t axis)
{
    switch (which) {
    case 0: /* v120_before_axis_same_flush: deliberately unequal encodings. */
        v120(axis, -30);
        continuous(axis, -20);
        finish_frame();
        break;
    case 1: /* axis_before_v120_same_flush */
        continuous(axis, -20);
        v120(axis, -30);
        finish_frame();
        break;
    case 2: /* v120_before_axis_button */
        v120(axis, -30);
        normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
        continuous(axis, -20);
        normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
        finish_frame();
        break;
    case 3: /* axis_before_v120_button: precision loss must be visible. */
        continuous(axis, -20);
        normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
        v120(axis, -30);
        normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
        finish_frame();
        break;
    case 4: /* between_buttons_v120_first: flush on release, not just press. */
        normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
        v120(axis, 90);
        continuous(axis, 20);
        normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
        finish_frame();
        break;
    case 5: /* between_buttons_axis_first */
        normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
        continuous(axis, 20);
        v120(axis, 90);
        normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
        finish_frame();
        break;
    case 6: /* v120_multiple_before_flush: neither delta is a snapshot. */
        v120(axis, 30);
        v120(axis, 60);
        finish_frame();
        break;
    case 7: /* v120_multiple_across_flush: two increments, one encoding. */
        v120(axis, 30);
        normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
        v120(axis, 60);
        normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
        continuous(axis, 20);
        finish_frame();
        break;
    case 8: /* axis_multiple_before_flush: frame's combined motion vector. */
        continuous(axis, 5);
        continuous(axis, 15);
        finish_frame();
        break;
    case 9: /* axis_multiple_across_flush_late_v120 */
        continuous(axis, 5);
        normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
        v120(axis, 90);
        continuous(axis, 15);
        normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
        finish_frame();
        break;
    case 10: /* stop_before_late_v120: a zero stop commits no distance. */
        stop_axis(axis);
        normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
        v120(axis, 30);
        continuous(axis, 20);
        normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
        finish_frame();
        break;
    case 11: /* frame_reset: v120 -> continuous -> v120 -> continuous. */
        v120(axis, 30);
        normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
        continuous(axis, 20);
        normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
        finish_frame();
        continuous(axis, -5);
        normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
        v120(axis, -120);
        normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
        finish_frame();
        continuous(axis, 10);
        v120(axis, 60);
        finish_frame();
        continuous(axis, -15);
        finish_frame();
        stop_axis(axis);
        finish_frame();
        finish_frame(); /* Defensive empty-frame no-output control. */
        v120(axis, 90);
        finish_frame();
        break;
    default:
        fixture_fail("unknown extended single-axis case");
    }
}

static void
extended_dual_axis(unsigned which)
{
    const uint32_t v = WL_POINTER_AXIS_VERTICAL_SCROLL;
    const uint32_t h = WL_POINTER_AXIS_HORIZONTAL_SCROLL;

    switch (which) {
    case 0: /* dual_order_same_flush */
        continuous(v, -20);
        v120(h, 60);
        v120(v, -30);
        continuous(h, 5);
        finish_frame();
        break;
    case 1: /* dual_independent_choices: swap the representations next frame. */
        continuous(v, -20);
        v120(h, 60);
        normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
        v120(v, -30);
        continuous(h, 5);
        normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
        finish_frame();
        v120(v, -60);
        continuous(h, 20);
        normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
        continuous(v, -5);
        v120(h, 30);
        normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
        finish_frame();
        break;
    case 2: /* dual_stop_other_axis: zero and nonzero axes remain independent. */
        stop_axis(v);
        continuous(h, 15);
        normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
        v120(v, -30);
        continuous(v, -20);
        normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
        finish_frame();
        stop_axis(h);
        continuous(v, -15);
        normal_button(WL_POINTER_BUTTON_STATE_PRESSED);
        v120(h, 60);
        continuous(h, 20);
        normal_button(WL_POINTER_BUTTON_STATE_RELEASED);
        finish_frame();
        break;
    case 3: /* dual_multiple_before_flush */
        continuous(v, 5);
        continuous(h, -5);
        continuous(v, 15);
        continuous(h, -10);
        finish_frame();
        break;
    default:
        fixture_fail("unknown extended dual-axis case");
    }
}

static void
run_extended_cases(void)
{
    static const char *const single_names[] = {
        "v120_before_axis_same_flush", "axis_before_v120_same_flush",
        "v120_before_axis_button", "axis_before_v120_button",
        "between_buttons_v120_first", "between_buttons_axis_first",
        "v120_multiple_before_flush", "v120_multiple_across_flush",
        "axis_multiple_before_flush", "axis_multiple_across_flush_late_v120",
        "stop_before_late_v120", "frame_reset"
    };
    static const char *const dual_names[] = {
        "dual_order_same_flush", "dual_independent_choices",
        "dual_stop_other_axis", "dual_multiple_before_flush"
    };
    unsigned gate, which, axis;
    char name[96];

    for (gate = 0; gate <= 1; ++gate) {
        for (which = 0; which < sizeof(single_names) / sizeof(single_names[0]); ++which) {
            for (axis = 0; axis < 2; ++axis) {
                snprintf(name, sizeof(name), "%s_%s", single_names[which],
                         axis ? "horizontal" : "vertical");
                fixture_begin(name, 8, gate);
                extended_single_axis(which, axis ? WL_POINTER_AXIS_HORIZONTAL_SCROLL :
                                                    WL_POINTER_AXIS_VERTICAL_SCROLL);
                fixture_end();
            }
        }
        for (which = 0; which < sizeof(dual_names) / sizeof(dual_names[0]); ++which) {
            fixture_begin(dual_names[which], 8, gate);
            extended_dual_axis(which);
            fixture_end();
        }
    }
}
#endif
