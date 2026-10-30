/* Exercise the real ext failure callback; substitute only transport boundaries. */
#include "src/screencast/ext_image_copy.c"

static unsigned destroyed, queued, triggered;
static char fake_frame;

void logprint(enum LOGLEVEL level, char *msg, ...) {}
// The retained protocol listener references this unused setup callback.
void pwr_update_stream_param(struct xdpw_screencast_instance *cast) { abort(); }
uint32_t wl_proxy_get_version(struct wl_proxy *proxy) { return 1; }
struct wl_proxy *wl_proxy_marshal_flags(struct wl_proxy *proxy, uint32_t opcode,
		const struct wl_interface *interface, uint32_t version, uint32_t flags, ...) {
	assert(proxy == (void *)&fake_frame);
	assert(flags & WL_MARSHAL_FLAG_DESTROY);
	destroyed++;
	return NULL;
}
void xdpw_screencast_instance_destroy(struct xdpw_screencast_instance *cast) {
	abort(); // A constraint change must remain recoverable.
}
void xdpw_pwr_enqueue_buffer(struct xdpw_screencast_instance *cast) {
	assert(destroyed == 1 && cast->ext_session.frame == NULL);
	cast->current_frame.pw_buffer = NULL;
	cast->current_frame.xdpw_buffer = NULL;
	queued++;
}
int pw_stream_trigger_process(struct pw_stream *stream) {
	assert(queued == 1);
	triggered++;
	return 0;
}

static void failed(bool streaming, bool destroying, unsigned expected_triggers) {
	struct xdpw_screencast_instance cast = {0};
	cast.pwr_stream_state = streaming;
	cast.destroying = destroying;
	cast.ext_session.frame = (void *)&fake_frame;
	cast.current_frame.damage.size = sizeof(struct xdpw_frame_damage);
	destroyed = queued = triggered = 0;
	ext_frame_failed(&cast, cast.ext_session.frame,
		EXT_IMAGE_COPY_CAPTURE_FRAME_V1_FAILURE_REASON_BUFFER_CONSTRAINTS);
	assert(destroyed == 1 && queued == 1 && triggered == expected_triggers);
	assert(cast.current_frame.damage.size == 0);
}

int main(void) {
	failed(true, false, 1); // Continue the driver graph after resize failure.
	failed(false, false, 0); // Pause owns the later restart.
	failed(true, true, 0); // Teardown never schedules new work.
	return 0;
}
