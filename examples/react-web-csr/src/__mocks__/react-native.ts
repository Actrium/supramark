export const StyleSheet = { create: () => ({}) };
export const View = 'View';
export const Text = 'Text';
export const ScrollView = 'ScrollView';
export const FlatList = 'FlatList';
export const Image = 'Image';
export const TouchableOpacity = 'TouchableOpacity';
export const TouchableHighlight = 'TouchableHighlight';
export const TextInput = 'TextInput';
export const Modal = 'Modal';
export const KeyboardAvoidingView = 'KeyboardAvoidingView';
export const Platform = {
  OS: 'web',
  select: (obj: Record<string, unknown>) => obj.web ?? Object.values(obj)[0],
};
export const Animated = {
  View: 'Animated.View',
  Text: 'Animated.Text',
  createAnimatedComponent: <T>(c: T) => c,
  timing: () => ({ start: () => {} }),
  spring: () => ({ start: () => {} }),
  loop: () => ({ start: () => {} }),
};
export const NativeModules = {};
export const PermissionsAndroid = { request: () => Promise.resolve('granted') };
export const BackHandler = { addEventListener: () => ({}), removeEventListener: () => {} };
export default {
  StyleSheet,
  View,
  Text,
  ScrollView,
  FlatList,
  Image,
  TouchableOpacity,
  TouchableHighlight,
  TextInput,
  Modal,
  KeyboardAvoidingView,
  Platform,
  Animated,
  NativeModules,
  PermissionsAndroid,
  BackHandler,
};
