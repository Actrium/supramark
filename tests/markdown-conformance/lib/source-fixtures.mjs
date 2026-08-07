export function explicitInputConfigs(sourceConfig) {
  const result = [];
  if (sourceConfig.input) result.push({ path: sourceConfig.input });
  if (sourceConfig.inputs) result.push(...sourceConfig.inputs);
  return result;
}

export function inputGlobConfigs(sourceConfig) {
  return sourceConfig.inputGlobs ?? [];
}

export function matchesInputGlob(filePath, inputGlob) {
  if (!globToRegExp(inputGlob.pattern).test(filePath)) return false;
  return !(inputGlob.exclude ?? []).some(pattern => globToRegExp(pattern).test(filePath));
}

export function isConfiguredInputPath(filePath, sourceConfig) {
  if (explicitInputConfigs(sourceConfig).some(input => input.path === filePath)) return true;
  return inputGlobConfigs(sourceConfig).some(inputGlob => matchesInputGlob(filePath, inputGlob));
}

export function globToRegExp(pattern) {
  if (typeof pattern !== 'string' || pattern.length === 0 || pattern.includes('\\')) {
    throw new Error(`Invalid repository glob: ${pattern}`);
  }

  let source = '^';
  for (let index = 0; index < pattern.length; index += 1) {
    const character = pattern[index];
    if (character === '*') {
      if (pattern[index + 1] === '*') {
        index += 1;
        if (pattern[index + 1] === '/') {
          index += 1;
          source += '(?:.*/)?';
        } else {
          source += '.*';
        }
      } else {
        source += '[^/]*';
      }
    } else if (character === '?') {
      source += '[^/]';
    } else {
      source += character.replace(/[.\\+^$(){}|[\]]/g, '\\$&');
    }
  }
  return new RegExp(`${source}$`);
}
