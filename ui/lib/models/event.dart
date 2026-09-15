class Event {
  final String id;
  final String rawText;
  final DateTime recordedAt;
  final String source;
  final String? status;
  final EventAnalysis? analysis;

  Event({
    required this.id,
    required this.rawText,
    required this.recordedAt,
    required this.source,
    this.status,
    this.analysis,
  });

  // Convert from Rust EventDto (will be used once bridge is generated)
  factory Event.fromRust(dynamic rustEvent) {
    return Event(
      id: rustEvent.id as String,
      rawText: rustEvent.rawText as String,
      recordedAt: DateTime.parse(rustEvent.recordedAt as String),
      source: rustEvent.source as String,
      status: rustEvent.status as String?,
    );
  }

  factory Event.fromJson(Map<String, dynamic> json) {
    return Event(
      id: json['id'] as String,
      rawText: json['raw_text'] as String,
      recordedAt: DateTime.parse(json['recorded_at'] as String),
      source: json['source'] as String,
      status: json['status'] as String?,
      analysis: json['analysis'] != null
          ? EventAnalysis.fromJson(json['analysis'] as Map<String, dynamic>)
          : null,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'raw_text': rawText,
      'recorded_at': recordedAt.toIso8601String(),
      'source': source,
      if (status != null) 'status': status,
      if (analysis != null) 'analysis': analysis!.toJson(),
    };
  }
}

class EventAnalysis {
  final String summary;
  final List<String>? tags;
  final Map<String, dynamic>? metadata;

  EventAnalysis({
    required this.summary,
    this.tags,
    this.metadata,
  });

  factory EventAnalysis.fromJson(Map<String, dynamic> json) {
    return EventAnalysis(
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

