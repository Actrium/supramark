import { registerRootComponent } from 'expo';

const App =
  process.env.EXPO_PUBLIC_SUPRAMARK_RN_E2E === 'selection'
    ? require('./SelectionE2EApp').default
    : require('./App').default;

// Use the registration method provided by expo to mount App as the "main" root component.
registerRootComponent(App);
