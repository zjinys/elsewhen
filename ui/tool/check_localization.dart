// Application literals only: user data is never rewritten by this check.
import 'dart:convert';
import 'dart:io';

import 'package:analyzer/dart/analysis/utilities.dart';
import 'package:analyzer/dart/ast/ast.dart';
import 'package:analyzer/dart/ast/visitor.dart';

final _han = RegExp(r'[\u3400-\u9fff]');

class _Inventory extends RecursiveAstVisitor<void> {
  _Inventory(this.path, this.source, this.rows);
  final String path, source;
  final List<Map<String, Object>> rows;
  void collect(StringLiteral node) {
    if (node.parent is AdjacentStrings || node.parent is StringInterpolation) {
      return;
    }
    var index = 0;
    String template(StringLiteral literal) {
      if (literal is SimpleStringLiteral) return literal.value;
      if (literal is AdjacentStrings) {
        return literal.strings.map(template).join();
      }
      if (literal is StringInterpolation) {
        return literal.elements
            .map(
              (part) =>
                  part is InterpolationString ? part.value : '{arg${index++}}',
            )
            .join();
      }
      return literal.toSource();
    }

    final text = template(node);
    if (_han.hasMatch(text)) {
      rows.add({
        'path': path,
        'text': text,
        'line': source.substring(0, node.offset).split('\n').length,
      });
    }
    // Interpolated expressions may themselves contain UI literals.
    void nested(StringLiteral literal) {
      if (literal is AdjacentStrings) {
        literal.strings.forEach(nested);
      } else if (literal is StringInterpolation) {
        for (final part
            in literal.elements.whereType<InterpolationExpression>()) {
          part.expression.accept(this);
        }
      }
    }

    nested(node);
  }

  @override
  void visitSimpleStringLiteral(SimpleStringLiteral node) => collect(node);
  @override
  void visitAdjacentStrings(AdjacentStrings node) => collect(node);
  @override
  void visitStringInterpolation(StringInterpolation node) => collect(node);
}

/// The AST preserves interpolation, adjacent strings and nested expressions.
List<Map<String, Object>> inventorySource(String path, String source) {
  final parsed = parseString(content: source, throwIfDiagnostics: false);
  if (parsed.errors.isNotEmpty) {
    throw FormatException('$path: cannot audit invalid Dart syntax');
  }
  final rows = <Map<String, Object>>[];
  parsed.unit.accept(_Inventory(path, source, rows));
  return rows;
}

String _id(Map<String, dynamic> row) => '${row['path']}\u0000${row['text']}';
List<String> checkOwnership(
  List<Map<String, dynamic>> rows,
  List<Map<String, dynamic>> exceptions,
) {
  final errors = <String>[];
  final actual = <String, int>{};
  for (final row in rows) {
    actual.update(_id(row), (n) => n + 1, ifAbsent: () => 1);
  }
  final allowed = <String, int>{};
  for (final row in exceptions) {
    if (row['path'] is! String ||
        row['text'] is! String ||
        (row['reason'] as String? ?? '').trim().isEmpty ||
        !{
          'protocol',
          'audit',
          'identity',
          'diagnostic',
        }.contains(row['category']) ||
        row['count'] is! int ||
        (row['count'] as int) < 1 ||
        allowed.containsKey(_id(row))) {
      errors.add('Invalid/duplicate localization exception: $row');
      continue;
    }
    allowed[_id(row)] = row['count'] as int;
  }
  for (final row in rows) {
    if (actual[_id(row)] != allowed[_id(row)]) {
      errors.add(
        '${row['path']}:${row['line']}: unowned application literal: ${row['text']}',
      );
    }
  }
  for (final id in allowed.keys) {
    if (allowed[id] != actual[id]) {
      errors.add(
        'Stale localization exception: ${id.replaceAll('\u0000', ': ')}',
      );
    }
  }
  return errors;
}

void main(List<String> args) {
  final rows = <Map<String, Object>>[];
  for (final file in Directory(
    'lib',
  ).listSync(recursive: true).whereType<File>()) {
    final path = file.path.replaceAll('\\', '/');
    if (!path.endsWith('.dart') ||
        path.contains('/generated.dart/') ||
        RegExp(r'/l10n/app_localizations(?:_\w+)?\.dart$').hasMatch(path) ||
        path.endsWith('.g.dart')) {
      continue;
    }
    try {
      rows.addAll(inventorySource('ui/$path', file.readAsStringSync()));
    } on FormatException catch (error) {
      stderr.writeln(error.message);
      exitCode = 1;
    }
  }
  rows.sort(
    (a, b) =>
        '${a['path']}:${a['line']}'.compareTo('${b['path']}:${b['line']}'),
  );
  if (args.length == 2 && args.first == '--inventory') {
    File(args[1]).writeAsStringSync(
      '${const JsonEncoder.withIndent('  ').convert(rows)}\n',
    );
    stdout.writeln('${rows.length} application literals inventoried');
    return;
  }
  final exceptions = (jsonDecode(
    File('../scripts/localization-exceptions.json').readAsStringSync(),
  ) as List).cast<Map<String, dynamic>>();
  final errors = checkOwnership(rows, exceptions);
  for (final error in errors) {
    stderr.writeln(error);
  }
  if (errors.isNotEmpty) exitCode = 1;
  if (exitCode == 0) {
    stdout.writeln(
      'Localization coverage: ${rows.length} explicit protocol/audit/identity/diagnostic exceptions; no unowned literals',
    );
  }
}
