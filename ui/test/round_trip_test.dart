// M2 §5.1 往返保真常驻测试：md ⇄ JSON ⇄ md 快照锁。
//
// 三层门（按 fixture 名称在 _gates 注册）：
//   bytes       —— t1(源) == t1(往返输出)，逐字节一致
//   spaced      —— 允许「块间空行插入/删除」类空白规范，但语义必须相等（AST 级）
//   knownDrift  —— 已知语义漂移，锁定字节快照 + 原因，任何变化都会红
//
// 任何新 fixture 未在 _gates 注册一律按 bytes 断言，防止悄悄引入新漂移。
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:markdown/markdown.dart' as md;

import 'package:elsewhen_ui/wiki/wiki_markdown_codec.dart';

const _fixtureDir = 'test/fixtures/wiki_md';

enum Gate { bytes, spaced, knownDrift }

/// fixture 门控清单。缺省 bytes；只有语义可证等价/已知取舍才放行。
const _gates = <String, Gate>{
  // —— 合成特征语料 ——
  's04_nested_ul': Gate.spaced, // 嵌套缩进 2 格 → tab：宽度归一，AST 等价
  's05_nested_ol_ul': Gate.spaced, // 嵌套缩进 3 格 → tab：宽度归一，AST 等价
  's16_two_lists': Gate.knownDrift, // 节点模型不保留列表边界：两个紧邻同型列表合并
  // —— 真实素材 ——
  'real-kb-qian': Gate.spaced, // 标题接段落空行归一
  'real-kb-shipin': Gate.spaced,
  'real-note': Gate.spaced,
  'real-person-liu': Gate.spaced, // 尾随空格(hard break)归一
  'real-tw-long-0ol': Gate.spaced, // 「0. 编号」保持；空行/尾随空格归一
  'real-tw-long-2': Gate.spaced,
  'real-tw-star': Gate.spaced,
};

/// knownDrift 快照：锁定往返输出字节；语义失败的 fixture 必须出现在此，
/// 且快照不得改变（代码c升级导致输出变化 → 红）。
const _knownDriftSnapshots = <String, String>{
  // 两个紧邻同型列表在节点模型里没有边界信息，往返会合并为单列表。
  's16_two_lists': '- 星号无序列表\n- 加号无序列表\n',
};

/// 生产管线：wiki codec（解码注册 WikilinkInlineSyntax，§5.2）。
String roundTrip(String md) => wikiDocumentToMarkdown(wikiMarkdownToDocument(md));

/// 仅归一化文件尾换行（\n+ 结尾 → 单个 \n；无结尾 → 补 \n）。
String _t1(String s) => '${s.replaceAll(RegExp(r'\n+$'), '')}\n';

/// 行级 LCS diff（- 原 + 现）；identical 行不输出。
List<String> _diffLines(String a, String b) {
  final la = a.split('\n');
  final lb = b.split('\n');
  final n = la.length, m = lb.length;
  final dp = List.generate(n + 1, (_) => List<int>.filled(m + 1, 0));
  for (var i = n - 1; i >= 0; i--) {
    for (var j = m - 1; j >= 0; j--) {
      dp[i][j] = la[i] == lb[j]
          ? dp[i + 1][j + 1] + 1
          : (dp[i + 1][j] > dp[i][j + 1] ? dp[i + 1][j] : dp[i][j + 1]);
    }
  }
  final out = <String>[];
  var i = 0, j = 0;
  while (i < n && j < m) {
    if (la[i] == lb[j]) {
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      out.add('- ${la[i++]}');
    } else {
      out.add('+ ${lb[j++]}');
    }
  }
  while (i < n) {
    out.add('- ${la[i++]}');
  }
  while (j < m) {
    out.add('+ ${lb[j++]}');
  }
  return out;
}

/// AST 序列化：文本节点折叠空白（代码块内原文保留），元素只保留 tag/class。
String _ast(md.Node n, {bool inCode = false}) {
  if (n is md.Text) {
    return inCode ? n.text : n.text.replaceAll(RegExp(r'\s+'), ' ').trim();
  }
  if (n is md.Element) {
    final tag = n.tag;
    final cls = n.attributes['class'];
    final kids = (n.children ?? const [])
        .map((c) => _ast(c, inCode: inCode || tag == 'pre' || tag == 'code'))
        .join('<');
    return '<$tag${cls != null ? ' c=$cls' : ''}>$kids';
  }
  return '?';
}

bool _semanticallyEqual(String a, String b) {
  String parse(String s) => md
      .Document(extensionSet: md.ExtensionSet.gitHubFlavored, encodeHtml: false)
      .parse(s)
      .map((n) => _ast(n))
      .join('</>');
  return parse(a) == parse(b);
}

void main() {
  final dir = Directory(_fixtureDir);
  final files = dir
      .listSync()
      .whereType<File>()
      .where((f) => f.path.endsWith('.md'))
      .toList()
    ..sort((a, b) => a.path.compareTo(b.path));

  test('fixture 目录非空', () {
    expect(files, isNotEmpty, reason: 'fixture 目录不应为空');
  });
  if (files.isEmpty) {
    return;
  }

  for (final f in files) {
    final name = f.uri.pathSegments.last.replaceAll('.md', '');
    final source = f.readAsStringSync();
    final gate = _gates[name] ?? Gate.bytes;

    test('round-trip $name [${gate.name}]', () {
      final out = roundTrip(source);
      final exact = _t1(source) == _t1(out);
      final semanticOk = _semanticallyEqual(source, out);
      final diff = _diffLines(_t1(source), _t1(out));

      switch (gate) {
        case Gate.bytes:
          expect(exact, isTrue,
              reason: '$name 必须逐字节收敛，实际漂移：\n${diff.join('\n')}');
        case Gate.spaced:
          expect(semanticOk, isTrue,
              reason: '$name（spaced 门）语义必须相等：\n${diff.join('\n')}');
        case Gate.knownDrift:
          final snapshot = _knownDriftSnapshots[name];
          expect(snapshot, isNotNull, reason: '$name 缺 knownDrift 快照');
          expect(_t1(out), _t1(snapshot!),
              reason: '$name 快照漂移（行为不应改变）：\n${diff.join('\n')}');
          expect(semanticOk, isFalse,
              reason: '$name 应被记录为语义漂移（若已收敛请移出 knownDrift）');
      }
    });
  }
}