/* Orders one of the local player's units to move 320 pixels sideways and shows what happened:
   submit_move <pid> [unit_type]
   Needs a running match in a game that runs the in-game DLL (bwrapi.dll injected).
   Build in a Visual Studio developer prompt, <zip> being the unpacked bwrapi-windows.zip:
     x64 prompt: cl /I<zip>\include submit_move.c <zip>\x64\bwrapi.lib
     x86 prompt: cl /I<zip>\include submit_move.c <zip>\x86\bwrapi.lib
   Run it with the bwrapi.dll of the same folder as the .lib beside submit_move.exe. */
#define _DEFAULT_SOURCE /* usleep */
#include <stdio.h>
#include <stdlib.h>

#ifdef _WIN32
#include <windows.h>
#define pause_ms(ms) Sleep(ms)
#else
#include <unistd.h>
#define pause_ms(ms) usleep((ms) * 1000)
#endif

#include "bwrapi.h"

#define SOURCES (BWR_SOURCE_PLAYERS | BWR_SOURCE_UNITS)

static const char* state_name(uint32_t state) {
    static const char* names[] = {"PENDING", "OK", "REJECTED", "UNAVAILABLE", "NOT_EXECUTED", "PANIC", "UNKNOWN"};
    return state <= BWR_RESULT_UNKNOWN ? names[state] : "?";
}

/* Row of the unit `id` in the frame, or -1. */
static long find_unit(const BwrFrame* frame, uint64_t id) {
    size_t row;
    for (row = 0; row < frame->units->rows; row++) {
        if (frame->units->id[row] == id) {
            return (long)row;
        }
    }
    return -1;
}

int main(int argc, char** argv) {
    BwrSession* session;
    const BwrFrame* frame;
    BwrStatus status;
    BwrCommand command = {0};
    BwrResult result = {0};
    uint64_t unit = 0, id = 0;
    uint32_t start_frame, tries;
    int32_t x = 0, y = 0, target_x;
    long row;
    size_t i;

    if (argc < 2) {
        fprintf(stderr, "usage: submit_move <pid> [unit_type]\n");
        return 2;
    }
    status = bwr_open((uint32_t)strtoul(argv[1], NULL, 10), 0, &session);
    if (status != BWR_OK) {
        fprintf(stderr, "open: %s\n", bwr_last_error(NULL));
        return 1;
    }
    bwr_subscribe(session, SOURCES);
    bwr_step(session, 1000);
    status = bwr_acquire(session, &frame);
    if (status != BWR_OK) {
        fprintf(stderr, "acquire: %s\n", bwr_last_error(session));
        bwr_close(session);
        return 1;
    }
    if (frame->units == NULL) { /* a subscribed source that could not be read */
        fprintf(stderr, "frame %u carries no units\n", frame->game_frame);
        bwr_release(session, frame);
        bwr_close(session);
        return 1;
    }
    for (i = 0; i < frame->units->rows; i++) {
        if (frame->units->owner[i] == frame->local_player && frame->units->completed[i] &&
            (argc < 3 || frame->units->unit_type[i] == (uint16_t)strtoul(argv[2], NULL, 10))) {
            unit = frame->units->id[i];
            x = frame->units->x[i];
            y = frame->units->y[i];
            break;
        }
    }
    start_frame = frame->game_frame;
    bwr_release(session, frame);
    if (unit == 0) {
        fprintf(stderr, "no local unit found\n");
        bwr_close(session);
        return 1;
    }
    /* Toward the map's middle side, so the target is on the map. */
    target_x = x > 320 ? x - 320 : x + 320;
    printf("frame %u: unit %llu at %d,%d -> move to %d,%d\n", start_frame, (unsigned long long)unit, x, y, target_x, y);

    command.size = sizeof(command);
    command.kind = BWR_COMMAND_MOVE;
    command.x = target_x;
    command.y = y;
    command.units = &unit;
    command.unit_count = 1;
    status = bwr_submit(session, &command, 1, &id);
    if (status != BWR_OK) {
        fprintf(stderr, "submit: status %d: %s\n", (int)status, bwr_last_error(session));
        bwr_close(session);
        return 1;
    }
    for (tries = 0; tries < 400; tries++) { /* up to 2 s */
        bwr_poll_result(session, id, &result);
        if (result.state != BWR_RESULT_PENDING) {
            break;
        }
        pause_ms(5);
    }
    printf("result: %s (sent for frame %u, actions %u)\n", state_name(result.state), result.frame_hint, result.actions);
    if (result.state != BWR_RESULT_OK) {
        fprintf(stderr, "%s\n", bwr_last_error(session));
    }

    /* Let the game run about 24 frames, then look at the same unit again. */
    for (tries = 0; tries < 200; tries++) {
        if (bwr_step(session, 100) != BWR_OK) {
            break;
        }
        if (bwr_acquire(session, &frame) != BWR_OK) {
            break;
        }
        if (frame->game_frame >= start_frame + 24) {
            row = frame->units != NULL ? find_unit(frame, unit) : -1;
            if (row >= 0) {
                printf("frame %u: order %u at %d,%d\n", frame->game_frame, frame->units->order[row],
                       frame->units->x[row], frame->units->y[row]);
            } else {
                printf("frame %u: unit is gone\n", frame->game_frame);
            }
            bwr_release(session, frame);
            break;
        }
        bwr_release(session, frame);
    }
    bwr_close(session);
    return result.state == BWR_RESULT_OK ? 0 : 1;
}
