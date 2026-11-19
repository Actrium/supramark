// The native CLI loads PowerShell on import. Metro only needs the platform
// mapping when bundling JavaScript on a non-Windows build host.
module.exports =
  process.platform === 'win32'
    ? {}
    : {
        platforms: {
          windows: {
            npmPackageName: 'react-native-windows',
            projectConfig: () => null,
            dependencyConfig: () => null,
          },
        },
      };
