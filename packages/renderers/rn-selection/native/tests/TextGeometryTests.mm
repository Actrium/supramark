#import "../macos/SupramarkTextGeometry.h"
#include <cassert>

static NSTextStorage *textStorage(NSString *text) {
  return [[NSTextStorage alloc] initWithString:text attributes:@{
    NSFontAttributeName: [NSFont systemFontOfSize:20]
  }];
}

int main() {
  @autoreleasepool {
    auto lines = SupramarkMeasureLines(textStorage(@"iiWW\nA😀Z"), CGRectMake(8, 10, 200, 100));
    assert(lines.count == 2);
    NSDictionary *first = lines[0];
    assert([first[@"text"] isEqualToString:@"iiWW\n"]);
    NSArray<NSNumber *> *edges = first[@"charXs"];
    assert(edges.count == 6);
    assert(edges[0].doubleValue == 8);
    assert(edges[3].doubleValue - edges[2].doubleValue > edges[1].doubleValue - edges[0].doubleValue);
    NSDictionary *second = lines[1];
    assert([second[@"text"] isEqualToString:@"A😀Z"]);
    assert([second[@"charXs"] count] == 5);
    assert([second[@"y"] doubleValue] > [first[@"y"] doubleValue]);
    auto wrapped = SupramarkMeasureLines(textStorage(@"First line with wrapped words"), CGRectMake(0, 0, 70, 220));
    assert(wrapped.count > 1);
    assert([[[wrapped valueForKey:@"text"] componentsJoinedByString:@""] isEqualToString:@"First line with wrapped words"]);
  }
}
