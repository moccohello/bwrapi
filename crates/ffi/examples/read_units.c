/* Prints the local player's units once: read_units <pid>.
   Build in a Visual Studio developer prompt, <zip> being the unpacked bwrapi-windows.zip:
     x64 prompt: cl /I<zip>\include read_units.c <zip>\x64\bwrapi.lib
     x86 prompt: cl /I<zip>\include read_units.c <zip>\x86\bwrapi.lib
   Run it with the bwrapi.dll of the same folder as the .lib beside read_units.exe. */
#include <stdio.h>
#include <stdlib.h>

#include "bwrapi.h"

int main(int argc, char** argv) {
    BwrSession* session;
    const BwrFrame* frame;
    BwrStatus status;
    size_t row;

    if (argc < 2) {
        fprintf(stderr, "usage: read_units <pid>\n");
        return 2;
    }
    status = bwr_open((uint32_t)strtoul(argv[1], NULL, 10), 0, &session);
    if (status != BWR_OK) {
        fprintf(stderr, "open: %s\n", bwr_last_error(NULL));
        return 1;
    }
    bwr_subscribe(session, BWR_SOURCE_UNITS);
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
    printf("frame %u: %zu units\n", frame->game_frame, frame->units->rows);
    for (row = 0; row < frame->units->rows; row++) {
        if (frame->units->owner[row] == frame->local_player) {
            printf("  %llu type %u at %d,%d\n", (unsigned long long)frame->units->id[row],
                   frame->units->unit_type[row], frame->units->x[row], frame->units->y[row]);
        }
    }
    bwr_release(session, frame);
    bwr_close(session);
    return 0;
}
