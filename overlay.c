// Loaded only into the scratchpad Alacritty (through DYLD_INSERT_LIBRARIES at launch). It puts the
// terminal window above the menu bar and sizes it to the whole screen, so the window covers the
// menu-bar strip without Alacritty switching to SimpleFullscreen. SimpleFullscreen is what changes
// the presentation options and makes the menu bar animate; a plain windowed terminal raised to this
// level never touches them. Re-applied whenever the window becomes key, so it survives hide and show
// and moving to another display. Injection works because the wrapper binary has no hardened runtime.
#include <Block.h>
#include <objc/message.h>
#include <objc/runtime.h>

typedef struct {
    double x, y, w, h;
} Rect;

static id nsstr(const char *s) {
    return ((id (*)(Class, SEL, const char *))objc_msgSend)(objc_getClass("NSString"), sel_registerName("stringWithUTF8String:"), s);
}

// winit keeps auxiliary windows besides the terminal; only the terminal carries the title we set
// with --title, so a window with an empty title is a helper and must be left at its own size.
static int is_terminal(id window) {
    id title = ((id (*)(id, SEL))objc_msgSend)(window, sel_registerName("title"));
    return title && ((unsigned long (*)(id, SEL))objc_msgSend)(title, sel_registerName("length")) > 0;
}

static void cover_screen(id window) {
    if (!window || !is_terminal(window)) return;
    // No scale/fade when the window is hidden or shown, so toggling is instant like on Linux.
    ((void (*)(id, SEL, long))objc_msgSend)(window, sel_registerName("setAnimationBehavior:"), 2); // NSWindowAnimationBehaviorNone
    ((void (*)(id, SEL, long))objc_msgSend)(window, sel_registerName("setLevel:"), 25); // above NSMainMenuWindowLevel (24)
    id screen = ((id (*)(id, SEL))objc_msgSend)(window, sel_registerName("screen"));
    if (!screen) return;
    Rect frame = ((Rect (*)(id, SEL))objc_msgSend)(screen, sel_registerName("frame"));
    ((void (*)(id, SEL, Rect, signed char))objc_msgSend)(window, sel_registerName("setFrame:display:"), frame, 1);
}

__attribute__((constructor)) static void start(void) {
    id center = ((id (*)(Class, SEL))objc_msgSend)(objc_getClass("NSNotificationCenter"), sel_registerName("defaultCenter"));
    void (^handler)(id) = ^(id note) {
        cover_screen(((id (*)(id, SEL))objc_msgSend)(note, sel_registerName("object")));
    };
    id block = (id)Block_copy(handler);
    SEL add = sel_registerName("addObserverForName:object:queue:usingBlock:");
    ((id (*)(id, SEL, id, id, id, id))objc_msgSend)(center, add, nsstr("NSWindowDidBecomeKeyNotification"), (id)0, (id)0, block);
    ((id (*)(id, SEL, id, id, id, id))objc_msgSend)(center, add, nsstr("NSWindowDidChangeScreenNotification"), (id)0, (id)0, block);
}
