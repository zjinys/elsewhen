class Event {
  final String id;
  final String rawText;
  final DateTime recordedAt;
  final String source;
  final Analysis? analysis;

  Event({
    required this.id,
    required this.rawText,
    required this.recordedAt,
    required this.source,
    this.analysis,
  });

  factory Event.fromJson(Map<String, dynamic> json) {
    return Event(
      id: json['id'] as String,
      rawText: json['raw_text'] as String,
      recordedAt: DateTime.parse(json['recorded_at'] as String),
      source: json['source'] as String,
      analysis: json['analysis'] != null
          ? Analysis.fromJson(json['analysis'] as Map<String, dynamic>)
          : null,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'raw_text': rawText,
      'recorded_at': recordedAt.toIso8601String(),
      'source': source,
      'analysis': analysis?.toJson(),
    };
  }
}

class Analysis {
  final String summary;
  final List<String>? tags;
  final Map<String, dynamic>? metadata;

  Analysis({
    required this.summary,
    this.tags,
    this.metadata,
  });

  factory Analysis.fromJson(Map<String, dynamic> json) {
    return Analysis(
      summary: json['summary'] as String,
      tags: (json['tags'] as List?)?.cast<String>(),
      metadata: json['metadata'] as Map<String, dynamic>?,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'summary': summary,
      if (tags != null) 'tags': tags,
      if (metadata != null) 'metadata': metadata,
    };
  }
}
