#include <assert.h>
#include <stdbool.h>
#include <poll.h>
#include <stdint.h>
#include <sys/socket.h>
#include <unistd.h>
#include <wayland-client.h>
#define main portal_main
#include "src/core/main.c"
#undef main

static void done(void *data, struct wl_callback *callback, uint32_t serial) {
	bool *called = data;
	*called = true;
}

static const struct wl_callback_listener listener = { .done = done };

int main(void) {
	int sockets[2];
	assert(socketpair(AF_UNIX, SOCK_STREAM, 0, sockets) == 0);
	struct wl_display *display = wl_display_connect_to_fd(sockets[0]);
	assert(display != NULL);

	struct wl_callback *callback = wl_display_sync(display);
	uint32_t id = wl_proxy_get_id((struct wl_proxy *)callback);
	wl_callback_destroy(callback);

	// A late event for a destroyed frame is discarded by libwayland. It must
	// not keep the portal waiting while PipeWire needs to replace its buffers.
	uint32_t event[] = { id, (12 << 16) | 0, 1 };
	assert(write(sockets[1], event, sizeof(event)) == sizeof(event));
	struct pollfd pfd = { .fd = sockets[0], .events = POLLIN };
	assert(poll(&pfd, 1, 0) == 1);
	assert(xdpw_wl_dispatch(display) == 0);

	bool called = false;
	callback = wl_display_sync(display);
	wl_callback_add_listener(callback, &listener, &called);
	event[0] = wl_proxy_get_id((struct wl_proxy *)callback);
	// A partial message must not block the other event sources either.
	assert(write(sockets[1], event, 4) == 4);
	assert(xdpw_wl_dispatch(display) == 0);
	assert(!called);
	assert(write(sockets[1], (char *)event + 4, sizeof(event) - 4) == sizeof(event) - 4);
	assert(xdpw_wl_dispatch(display) > 0);
	assert(called);

	wl_callback_destroy(callback);
	wl_display_disconnect(display);
	close(sockets[1]);
	return 0;
}
