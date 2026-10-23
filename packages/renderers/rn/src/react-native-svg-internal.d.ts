/**
 * `react-native-svg`'s public entry re-exports `SvgXml` but not the XML parser
 * it is built on, and `svgAst.tsx` needs `parse` to decode text references
 * before they reach React. Point that internal import at the package's
 * published declarations: importing its `src/*` sources directly would pull
 * the un-compiled tree (and its React type mismatches) into this package's
 * type check.
 */
declare module 'react-native-svg/lib/commonjs/xml' {
  export * from 'react-native-svg/lib/typescript/xml';
}