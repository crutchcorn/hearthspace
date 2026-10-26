/* Synthetic PAM boundary fixture, loaded only in lock-auth-test's private
 * mount namespace. Never logs prompts, usernames, or credentials. */
#define _GNU_SOURCE
#include <fcntl.h>
#include <pwd.h>
#include <security/pam_modules.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static const char credential[] = "hearthspace private PAM fixture credential";

static int record(const char *event) {
    int fd = open("/etc/pam.d/.events", O_WRONLY | O_APPEND | O_NOFOLLOW | O_CLOEXEC);
    if (fd < 0)
        return PAM_SYSTEM_ERR;
    size_t length = strlen(event);
    ssize_t written = write(fd, event, length);
    int closed = close(fd);
    return written == (ssize_t)length && closed == 0 ? PAM_SUCCESS : PAM_SYSTEM_ERR;
}

#define RECORD(event) do { if (record(event "\n") != PAM_SUCCESS) return PAM_SYSTEM_ERR; } while (0)

static int converse(pam_handle_t *pamh, int style, int *matches) {
    const void *item = NULL;
    if (pam_get_item(pamh, PAM_CONV, &item) != PAM_SUCCESS || item == NULL)
        return PAM_SYSTEM_ERR;
    const struct pam_conv *conv = item;
    struct pam_message message = {style, "Synthetic PAM fixture message"};
    const struct pam_message *messages[] = {&message};
    struct pam_response *responses = NULL;
    int status = conv->conv(1, messages, &responses, conv->appdata_ptr);
    *matches = 0;
    if (responses != NULL) {
        if (responses[0].resp != NULL) {
            *matches = status == PAM_SUCCESS && responses[0].resp_retcode == 0 &&
                       strcmp(responses[0].resp, credential) == 0;
            explicit_bzero(responses[0].resp, strlen(responses[0].resp));
            free(responses[0].resp);
        }
        free(responses);
    }
    return status;
}

PAM_EXTERN int pam_sm_authenticate(pam_handle_t *pamh, int flags,
                                  int argc, const char **argv) {
    RECORD("auth");
    if (argc != 1 || flags != PAM_DISALLOW_NULL_AUTHTOK)
        return PAM_AUTH_ERR;
    RECORD("auth-flags");
    const char *user = NULL;
    struct passwd *actual = getpwuid(getuid());
    if (getuid() == 0 || getuid() != geteuid() || actual == NULL ||
        pam_get_user(pamh, &user, NULL) != PAM_SUCCESS || user == NULL ||
        strcmp(user, actual->pw_name) != 0)
        return PAM_AUTH_ERR;
    RECORD("uid-user");
    RECORD("echo-off");
    int matches = 0;
    if (converse(pamh, PAM_PROMPT_ECHO_OFF, &matches) != PAM_SUCCESS || !matches) {
        RECORD("credential-rejected");
        return PAM_AUTH_ERR;
    }
    RECORD("credential-accepted");
    if (strcmp(argv[0], "auth_userchange") == 0) {
        if (pam_set_item(pamh, PAM_USER, "hearthspace-fixture-substituted-user") != PAM_SUCCESS)
            return PAM_SYSTEM_ERR;
        RECORD("auth-user-change");
    } else if (strcmp(argv[0], "second_prompt") == 0) {
        int status = converse(pamh, PAM_PROMPT_ECHO_OFF, &matches);
        if (status == PAM_CONV_ERR) {
            RECORD("second-prompt-rejected");
        } else {
            RECORD("second-prompt-answered");
        }
    } else if (strcmp(argv[0], "echo_prompt") == 0) {
        int status = converse(pamh, PAM_PROMPT_ECHO_ON, &matches);
        if (status == PAM_CONV_ERR) {
            RECORD("echo-prompt-rejected");
        } else {
            RECORD("echo-prompt-answered");
        }
    } else if (strcmp(argv[0], "error_then_success") == 0) {
        if (converse(pamh, PAM_ERROR_MSG, &matches) != PAM_SUCCESS)
            return PAM_SYSTEM_ERR;
        RECORD("error-message");
    }
    // Deliberately ignore rejected extra prompts and return success. The locker
    // must retain conversation failure even when a PAM module ignores it.
    RECORD("auth-success");
    return PAM_SUCCESS;
}

PAM_EXTERN int pam_sm_acct_mgmt(pam_handle_t *pamh, int flags,
                               int argc, const char **argv) {
    RECORD("account");
    if (argc != 1 || flags != 0)
        return PAM_SYSTEM_ERR;
    RECORD("account-flags");
    if (strcmp(argv[0], "expired") == 0) {
        RECORD("account-expired");
        return PAM_ACCT_EXPIRED;
    }
    if (strcmp(argv[0], "new_authtok") == 0) {
        RECORD("account-new-authtok");
        return PAM_NEW_AUTHTOK_REQD;
    }
    if (strcmp(argv[0], "account_userchange") == 0) {
        if (pam_set_item(pamh, PAM_USER, "hearthspace-fixture-substituted-user") != PAM_SUCCESS)
            return PAM_SYSTEM_ERR;
        RECORD("account-user-change");
    }
    RECORD("account-success");
    return PAM_SUCCESS;
}

// Any accidental session, password, or credential operation must be visible.
#define FORBIDDEN(name) \
    PAM_EXTERN int name(pam_handle_t *pamh, int flags, int argc, const char **argv) { \
        (void)pamh; (void)flags; (void)argc; (void)argv; \
        RECORD("forbidden-operation"); \
        return PAM_SYSTEM_ERR; \
    }
FORBIDDEN(pam_sm_setcred)
FORBIDDEN(pam_sm_open_session)
FORBIDDEN(pam_sm_close_session)
FORBIDDEN(pam_sm_chauthtok)
