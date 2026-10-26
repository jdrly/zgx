// The gate's runs of a binary count the same instructions every time
// (gate.py preloads this into each, under valgrind; never into anything
// shipped). Without it the same binary counted up to 0.3% apart:
//
// - CLOCK_MONOTONIC: Luau's collector paces its cycles by measured
//   allocation rates and mark times, and a script call reads the thread's
//   CPU time only when its last reading is over a millisecond old. Here
//   the clock starts at the whole second after the process did (real
//   enough that a futex deadline made from it, std's park_timeout, is in
//   the future), and only the main thread moves it: `STEP` a read, and
//   what it sleeps. Other threads (the timer scheduler, the script
//   watchdog) read where it is. `STEP` is app-mail's native time between
//   the main thread's reads (26-29 us, measured without valgrind), so a
//   heuristic paced by reads (the CPU-time reading's age) runs as natively.
//   Luau's heap trigger compares a cycle's mark time with its allocation
//   time, which a clock moved by reads cannot measure (marking reads it
//   about never): here cycles start at the heap goal, and app-mail does
//   about 1% less collecting than on the real clock, at any step. To
//   measure a change to that pacing, count on the real clock:
//   `ZGX_REAL_CLOCK=1 python3 scripts/perf/gate.py BASE` (then exact to
//   about 0.3% in the Luau scenes).
// - getrandom: std's RandomState seeds every HashMap from it, and a map's
//   probes and growth move with the seed. Here it gives the same bytes.
//
// With ZGX_SAME_RUN_REPORT set, the reads and calls are written to stderr
// as the process exits: gate.py checks this was loaded.
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

#define STEP 25000ULL

static unsigned long long now;
static int real_clock;
static unsigned long clock_reads, random_calls;

static void report(void) {
    if (getenv("ZGX_SAME_RUN_REPORT"))
        fprintf(stderr, "same_run: %lu clock reads, %lu getrandom calls\n", clock_reads, random_calls);
}

__attribute__((constructor)) static void init(void) {
    struct timespec real;
    syscall(SYS_clock_gettime, CLOCK_MONOTONIC, &real);
    now = ((unsigned long long)real.tv_sec + 1) * 1000000000ULL;
    real_clock = getenv("ZGX_REAL_CLOCK") != NULL;
    atexit(report);
}

int clock_gettime(clockid_t clock, struct timespec *tp) {
    if (clock != CLOCK_MONOTONIC || real_clock)
        return (int)syscall(SYS_clock_gettime, clock, tp);
    __atomic_fetch_add(&clock_reads, 1, __ATOMIC_RELAXED);
    // Only the main thread moves it: what other threads read (a timer's
    // or the script watchdog's) never changes what it reads next.
    unsigned long long ns = syscall(SYS_gettid) == getpid()
        ? __atomic_add_fetch(&now, STEP, __ATOMIC_SEQ_CST)
        : __atomic_load_n(&now, __ATOMIC_SEQ_CST);
    tp->tv_sec = (time_t)(ns / 1000000000ULL);
    tp->tv_nsec = (long)(ns % 1000000000ULL);
    return 0;
}

// The main thread's sleeps move its clock on by what they asked for, as
// time would: a test app waiting a millisecond at a time for a timer
// another thread fires sees it pass.
static void slept(const struct timespec *asked) {
    if (syscall(SYS_gettid) == getpid())
        __atomic_add_fetch(&now, (unsigned long long)asked->tv_sec * 1000000000ULL + (unsigned long long)asked->tv_nsec,
                           __ATOMIC_SEQ_CST);
}

int nanosleep(const struct timespec *asked, struct timespec *left) {
    int r = (int)syscall(SYS_nanosleep, asked, left);
    slept(asked);
    return r;
}

int clock_nanosleep(clockid_t clock, int flags, const struct timespec *asked, struct timespec *left) {
    if ((clock != CLOCK_MONOTONIC && clock != CLOCK_REALTIME) || flags != 0)
        return (int)syscall(SYS_clock_nanosleep, clock, flags, asked, left);
    int r = (int)syscall(SYS_clock_nanosleep, clock, flags, asked, left);
    slept(asked);
    return r;
}

ssize_t getrandom(void *buf, size_t len, unsigned int flags) {
    (void)flags;
    __atomic_fetch_add(&random_calls, 1, __ATOMIC_RELAXED);
    unsigned char *out = buf;
    for (size_t i = 0; i < len; i++)
        out[i] = (unsigned char)(i * 131 + 7);
    return (ssize_t)len;
}
