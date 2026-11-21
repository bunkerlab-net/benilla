/* Original authored qualification fixture. MIT OR Apache-2.0. No game data. */
typedef unsigned long u64;
static inline long sc(long n, long a, long b, long c, long d, long e, long f) {
    register long r10 __asm__("r10") = d;
    register long r8 __asm__("r8") = e;
    register long r9 __asm__("r9") = f;
    long out;
    __asm__ volatile("syscall" : "=a"(out) : "a"(n), "D"(a), "S"(b), "d"(c), "r"(r10), "r"(r8), "r"(r9) : "rcx", "r11", "memory");
    return out;
}
void gate_one(void);
void gate_two(void);
void perturb(void);
long spawn(u64 flags, void *stack);
unsigned int mxcsr_value;
static volatile int done;
static volatile int hold_worker;
static volatile int release_worker;
static char thread_stack[16384] __attribute__((aligned(16)));
void worker(void) {
    done = 1;
    while (hold_worker && !release_worker) sc(24,0,0,0,0,0,0);
}
__asm__(
    ".global gate_one, gate_end, gate_two, perturb, perturb_end, spawn, _start\n"
    "gate_one: nop\nadd $1,%rax\nnop\ngate_end: nop\nret\n"
    "gate_two: nop\nret\n"
    "perturb: stmxcsr mxcsr_value(%rip)\norl $0x2000,mxcsr_value(%rip)\nldmxcsr mxcsr_value(%rip)\nperturb_end: nop\nret\n"
    "spawn: xor %edx,%edx\nxor %r10d,%r10d\nxor %r8d,%r8d\nmov $56,%eax\nsyscall\ntest %rax,%rax\njnz 1f\ncall worker\nxor %edi,%edi\nmov $60,%eax\nsyscall\n1: ret\n"
    "_start: mov %rsp,%rdi\nand $-16,%rsp\ncall fixture\nmov %eax,%edi\nmov $60,%eax\nsyscall\n"
);
static int file_ops(void) {
    const char name[] = "fixture.bin";
    const char data[] = { 'n', 'a', 't', 'i', 'v', 'e', 0, (char)0xff };
    char back[8];
    long fd = sc(257, -100, (long)name, 0102 | 01000, 0600, 0, 0);
    if (fd < 0) return 10;
    if (sc(1, fd, (long)data, 8, 0, 0, 0) != 8) return 11;
    if (sc(8, fd, 0, 0, 0, 0, 0) != 0) return 12;
    if (sc(0, fd, (long)back, 8, 0, 0, 0) != 8) return 13;
    for (int i = 0; i < 8; ++i) if (data[i] != back[i]) return 14;
    if (sc(0, fd, (long)back, 0, 0, 0, 0) != 0) return 15;
    if (sc(3, fd, 0, 0, 0, 0, 0) != 0) return 16;
    if (sc(257, -100, (long)"absent", 0, 0, 0, 0) != -2) return 17;
    return 0;
}
int fixture(u64 *stack) {
    char **argv = (char **)(stack + 1);
    char **env = argv + stack[0] + 1;
    const char *expected[] = { "LC_ALL=C", "DUP=one", "DUP=two", "RAW=\xff" };
    for (int i = 0; i < 4; ++i) {
        if (!env[i]) return 30;
        for (int j = 0;; ++j) {
            if ((unsigned char)env[i][j] != (unsigned char)expected[i][j]) return 31;
            if (!expected[i][j]) break;
        }
    }
    if (env[4]) return 32;
    /* Define fixture FP inputs, including in-use state, before either observer level. */
    __asm__ volatile("fninit; fld1; pcmpeqd %%xmm0,%%xmm0" ::: "xmm0");
    if (stack[0] > 2) {
        unsigned int profile = 0;
        for (const char *p = argv[2]; *p; ++p) profile = profile * 10 + (unsigned int)(*p - '0');
        unsigned short control = 0x037f;
        unsigned int mxcsr = 0x1f80;
        if (profile < 64) {
            control |= (profile & 3) << 10;
            mxcsr |= ((profile >> 2) & 3) << 13;
            if (profile & 16) mxcsr |= 1u << 6;
            if (profile & 32) mxcsr |= 1u << 15;
        } else {
            unsigned int sticky = profile == 70 ? 63 : 1u << (profile - 64);
            mxcsr |= sticky;
            unsigned char environment[28];
            __asm__ volatile("fnstenv %0" : "=m"(environment));
            environment[4] |= (unsigned char)sticky;
            __asm__ volatile("fldenv %0" :: "m"(environment));
        }
        __asm__ volatile("fldcw %0; ldmxcsr %1" :: "m"(control), "m"(mxcsr));
    }
    if (stack[0] > 1 && argv[1][0] == 'l') return 0;
    /* Every accepted fixture closes descriptors explicitly, before any worker exists. */
    sc(3,0,0,0,0,0,0);
    sc(3,1,0,0,0,0,0);
    sc(3,2,0,0,0,0,0);
    if (stack[0] > 1 && argv[1][0] == 'p') { perturb(); return 0; }
    if (stack[0] > 1 && argv[1][0] == 'u') { sc(110,0,0,0,0,0,0); return 0; }
    gate_one(); gate_two(); gate_one();
    if (stack[0] > 1 && argv[1][0] == 's') {
        hold_worker = 1;
        if (spawn(0x10f00, thread_stack + sizeof(thread_stack)) < 0) return 23;
        while (!done) sc(24,0,0,0,0,0,0);
        int result = file_ops();
        release_worker = 1;
        return result;
    }
    if (stack[0] > 1 && argv[1][0] == 't') {
        /* CLONE_VM|FS|FILES|SIGHAND|THREAD, source workers run concurrently. */
        if (spawn(0x10f00, thread_stack + sizeof(thread_stack)) < 0) return 20;
        while (!done) sc(24,0,0,0,0,0,0);
        /* No file operation reads shared mutable memory while worker is alive. */
        return 0;
    }
    if (stack[0] > 1 && argv[1][0] == 'f') {
        long child = sc(57,0,0,0,0,0,0);
        if (child < 0) return 21;
        if (child == 0) { gate_two(); return 0; }
        int status;
        if (sc(61,child,(long)&status,0,0,0,0) != child || status != 0) return 22;
    }
    return file_ops();
}
