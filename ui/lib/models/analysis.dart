class Analysis {
  final String eventType;
  final double confidence;
  final String summary;
  final List<String> clarifications;

  Analysis({
    required this.eventType,
    required this.confidence,
    required this.summary,
    required this.clarifications,
  });
}
