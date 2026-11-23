#import "SupramarkTextGeometry.h"
#include <algorithm>
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

// At a bidi run boundary one logical offset has two visual carets. TextKit's
// primary insertion points cover interiors. Its selection rectangles identify
// which of the two edges belongs to each run, including shaped ligatures.
static void addBidiEdges(NSMutableDictionary *line, NSLayoutManager *layout, NSTextContainer *container,
                         NSRange glyphs, NSRange chars, CGFloat insetX) {
  std::vector<unsigned char> levels(glyphs.length);
  NSUInteger count = [layout getGlyphsInRange:glyphs glyphs:nullptr properties:nullptr
                             characterIndexes:nullptr bidiLevels:levels.data()];
  if (std::none_of(levels.begin(), levels.begin() + count, [](unsigned char level) { return level & 1; })) return;
  NSMutableArray<NSNumber *> *starts = [line[@"charXs"] mutableCopy];
  NSMutableArray<NSNumber *> *ends = [[starts subarrayWithRange:NSMakeRange(1, chars.length)] mutableCopy];
  NSString *text = line[@"text"];
  NSUInteger visible = chars.length;
  while (visible && [[NSCharacterSet newlineCharacterSet] characterIsMember:[text characterAtIndex:visible - 1]]) --visible;
  NSRange visibleChars = NSMakeRange(chars.location, visible);
  for (NSUInteger i = 0; i < count;) {
    NSUInteger j = i + 1;
    while (j < count && levels[j] == levels[i]) ++j;
    NSRange run = NSMakeRange(glyphs.location + i, j - i);
    bool rtl = levels[i] & 1;
    i = j;
    NSRange range = NSIntersectionRange([layout characterRangeForGlyphRange:run actualGlyphRange:nullptr], visibleChars);
    if (!range.length) continue;
    run = [layout glyphRangeForCharacterRange:range actualCharacterRange:nullptr];
    __block NSRect bounds = NSZeroRect;
    [layout enumerateEnclosingRectsForGlyphRange:run withinSelectedGlyphRange:NSMakeRange(NSNotFound, 0)
        inTextContainer:container usingBlock:^(NSRect rect, BOOL *) { bounds = NSUnionRect(bounds, rect); }];
    if (NSIsEmptyRect(bounds)) continue;
    NSUInteger end = NSMaxRange(range) - chars.location;
    starts[range.location - chars.location] = @(insetX + (rtl ? NSMaxX(bounds) : NSMinX(bounds)));
    ends[end - 1] = @(insetX + (rtl ? NSMinX(bounds) : NSMaxX(bounds)));
    if (end == visible) starts[end] = ends[end - 1];
  }
  line[@"charXs"] = starts;
  line[@"charEndXs"] = ends;
}

NSArray *SupramarkMeasureLines(NSTextStorage *storage, CGRect frame) {
  NSLayoutManager *layout = [NSLayoutManager new];
  layout.usesFontLeading = NO;
  NSTextContainer *container = [[NSTextContainer alloc] initWithSize:frame.size];
  container.lineFragmentPadding = 0;
  // SelectableBlock renders unlimited Text without font fitting, matching
  // RCTTextLayoutManager's maximumNumberOfLines == 0 configuration.
  container.lineBreakMode = NSLineBreakByClipping;
  [layout addTextContainer:container];
  [storage addLayoutManager:layout];
  NSMutableArray *lines = [NSMutableArray new];
  __block NSUInteger paragraphEnd = 0;
  __block BOOL paragraphRTL = NO;
  [layout enumerateLineFragmentsForGlyphRange:[layout glyphRangeForTextContainer:container]
      usingBlock:^(NSRect fragment, NSRect used, NSTextContainer *, NSRange glyphs, BOOL *) {
    NSRange chars = [layout characterRangeForGlyphRange:glyphs actualGlyphRange:nullptr];
    if (!chars.length) return;
    NSArray *edges = caretPositions(layout, chars, NSOffsetRect(fragment, frame.origin.x, 0),
                                   NSOffsetRect(used, frame.origin.x, 0));
    if (chars.location >= paragraphEnd) {
      NSRange paragraph = [storage.string paragraphRangeForRange:NSMakeRange(chars.location, 0)];
      paragraphEnd = NSMaxRange(paragraph);
      NSParagraphStyle *style = [storage attribute:NSParagraphStyleAttributeName atIndex:chars.location effectiveRange:nullptr];
      // The primary insertion point at the start of the paragraph is on its
      // leading visual edge, even when the first run has the opposite direction.
      paragraphRTL = style && style.baseWritingDirection != NSWritingDirectionNatural
          ? style.baseWritingDirection == NSWritingDirectionRightToLeft
          : [edges.firstObject doubleValue] > NSMidX(used) + frame.origin.x;
    }
    NSMutableDictionary *line = [@{@"text": [storage.string substringWithRange:chars], @"charXs": edges,
        @"x": @(NSMinX(used) + frame.origin.x), @"y": @(NSMinY(used) + frame.origin.y),
        @"layoutX": @(NSMinX(used)), @"layoutY": @(NSMinY(used)), @"rtl": @(paragraphRTL),
        @"width": @(NSWidth(used)), @"height": @(NSHeight(used))} mutableCopy];
    addBidiEdges(line, layout, container, glyphs, chars, frame.origin.x);
    [lines addObject:line];
  }];
  return lines;
}
