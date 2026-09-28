enum AppMode {
  main, // Full application with timeline
  capture, // Quick capture floating window
}

class AppConfig {
  final AppMode mode;
  final String? databasePath;

  AppConfig({required this.mode, this.databasePath});

  factory AppConfig.fromArgs(List<String> args) {
    // Parse command line args: --mode=capture or --mode=main
    final mode = args.contains('--mode=capture')
        ? AppMode.capture
        : AppMode.main;

    return AppConfig(mode: mode);
  }
}
