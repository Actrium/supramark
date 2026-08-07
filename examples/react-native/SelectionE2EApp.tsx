import React from 'react';

import SelectionDemo from './SelectionDemo';

export default function SelectionE2EApp() {
  return <SelectionDemo e2e flatList scrollSentinel onBack={() => undefined} />;
}
