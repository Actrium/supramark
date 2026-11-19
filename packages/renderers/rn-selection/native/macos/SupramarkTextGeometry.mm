#import "SupramarkTextGeometry.h"
#include <vector>

static NSArray<NSNumber *> *caretPositions(NSLayoutManager *layout, NSRange range, NSRect fragment, NSRect used) {
  const NSUInteger count = [layout getLineFragmentInsertionPointsForCharacterAtIndex:range.location
      alternatePositions:NO inDisplayOrder:NO positions:nullptr characterIndexes:nullptr];
  std::vector<CGFloat> xs(count);
  std::vector<NSUInteger> indices(count);
  [layout getLineFragmentInsertionPointsForCharacterAtIndex:range.location alternatePositions:NO
      inDisplayOrder:NO positions:xs.data() characterIndexes:indices.data()];
  NSMutableArray<NSNumber *> *positions = [NSMutableArray arrayWithCapacity:range.length + 1];
  NSUInteger next = 0;
  CGFloat x = NSMinX(used);
  for (NSUInteger offset = 0; offset <= range.length; ++offset) {
    while (next < count && indices[next] <= range.location + offset) {
      x = NSMinX(fragment) + xs[next++];
    }
    if (offset == range.length && (count == 0 || indices.back() < NSMaxRange(range))) x = NSMaxX(used);
    [positions addObject:@(x)];
  }
  return positions;
}

NSArray *SupramarkMeasureLines(NSTextStorage *storage, CGRect frame) {
  NSLayoutManager *layout = [NSLayoutManager new];
  layout.usesFontLeading = NO;
  NSTextContainer *container = [[NSTextContainer alloc] initWithSize:frame.size];
  container.lineFragmentPadding = 0;
  container.lineBreakMode = NSLineBreakByClipping;
  [layout addTextContainer:container];
  [storage addLayoutManager:layout];
  NSMutableArray *lines = [NSMutableArray new];
  [layout enumerateLineFragmentsForGlyphRange:[layout glyphRangeForTextContainer:container]
      usingBlock:^(NSRect fragment, NSRect used, NSTextContainer *, NSRange glyphs, BOOL *) {
    NSRange chars = [layout characterRangeForGlyphRange:glyphs actualGlyphRange:nullptr];
    if (!chars.length) return;
    NSArray *edges = caretPositions(layout, chars, NSOffsetRect(fragment, frame.origin.x, 0),
                                   NSOffsetRect(used, frame.origin.x, 0));
    [lines addObject:@{@"text": [storage.string substringWithRange:chars], @"charXs": edges,
        @"x": @(NSMinX(used) + frame.origin.x), @"y": @(NSMinY(used) + frame.origin.y),
        @"width": @(NSWidth(used)), @"height": @(NSHeight(used))}];
  }];
  return lines;
}
