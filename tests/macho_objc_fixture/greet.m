// Tiny Objective-C fixture for the container::macho_objc reader.
// Exercises: one class with ivar + property + protocol + class
// method, one protocol with required/optional methods, one
// category on an *external* class (NSObject) — categories on
// classes defined in the same image are merged into the class
// method list by the linker and don't produce __objc_catlist.

#import <objc/NSObject.h>

@protocol Greeter
- (void)hello;
@optional
- (void)bye;
@end

@interface Greet : NSObject <Greeter> {
    int _count;
}
@property (nonatomic) int count;
- (void)hello;
- (void)bye;
+ (Greet *)shared;
@end

@implementation Greet
@synthesize count = _count;
- (void)hello { _count++; }
- (void)bye { _count--; }
+ (Greet *)shared { return nil; }
@end

// Category on NSObject — forces __objc_catlist emission.
@interface NSObject (Util)
- (int)util_value;
@end

@implementation NSObject (Util)
- (int)util_value { return 42; }
@end
