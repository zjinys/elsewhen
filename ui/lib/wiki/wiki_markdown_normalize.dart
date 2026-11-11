import 'package:appflowy_editor/appflowy_editor.dart';

/// 归一化剪贴板/存量 markdown 里的「表格块内空行」。
///
/// 背景：导入/粘贴链路（AppFlowy editor pastePlainText 每行一段落 +
/// encoder 块间空行）会把表格行与表头/分隔行之间垫出空行。GFM 规定
/// 表头行与分隔行必须相邻，空行隔断后整张表退化成 `|` 开头的普通段落，
/// 知识页浏览态就不渲染表格。
///
/// 处理：进入 `|` 开头的行序列后，吞掉其中的空行，让表头、分隔行、
/// 数据行连续。代码围栏（```）内的 `|` 行不动。单行 `|` 文本（非表格
/// 用法）吞掉后随空行并成一段，文本不丢，仍按段落降级展示。
///
/// 表格块终止条件：连续 `|` 行里出现「首个分隔行（`|---|`）已过，
/// 且后续 `|` 行数已达到「表头列数」」时收束——多出的行属于后续段落
/// 被 pastePlainText 平铺成行的普通文本，不能并进表格。
String normalizeTableBlanks(String markdown) {
  final lines = markdown.split('\n');
  final out = <String>[];
  var inFence = false;
  var i = 0;
  while (i < lines.length) {
    final line = lines[i];
    if (line.trimLeft().startsWith('```')) {
      inFence = !inFence;
      out.add(line);
      i++;
      continue;
    }
    if (inFence) {
      out.add(line);
      i++;
      continue;
    }
    final isPipe = line.trimLeft().startsWith('|');
    if (!isPipe) {
      out.add(line);
      i++;
      continue;
    }
    // 管道行序列：吞中间空行；遇到首个分隔行后统计列数，
    // 数据行收满列数即认为表格结束（避免吞掉后续段落行）。
    final buf = <String>[line];
    i++;
    var expectCols = -1;
    var dataRows = 0;
    var closed = false;
    while (i < lines.length) {
      final cur = lines[i];
      if (cur.trimLeft().startsWith('```')) {
        break; // 围栏：立即收束，表格判定交给解码器
      }
      if (cur.trim().isEmpty) {
        if (closed) break; // 表格已收束：空行归位，不再吞
        i++;
        continue;
      }
      if (!cur.trimLeft().startsWith('|')) {
        break;
      }
      final isSep = _isTableSepRow(cur);
      if (expectCols < 0) {
        if (isSep) {
          expectCols = _pipeCols(buf.last);
          buf.add(cur);
          i++;
          continue;
        }
        // 分隔行之前的管道行：无法确认是表格，全部吞空行连接
        buf.add(cur);
        i++;
        continue;
      }
      // 已过分隔行：数据行
      if (dataRows >= expectCols) {
        closed = true;
        break;
      }
      buf.add(cur);
      dataRows++;
      i++;
    }
    out.addAll(buf);
  }
  return out.join('\n');
}

/// 分隔行判定：整行由 `|`、`-`、`:` 与空白组成，且至少含一个 `-`。
bool _isTableSepRow(String line) {
  final s = line.trim();
  if (!s.contains('-')) return false;
  return s.split('').every((c) => c == '|' || c == '-' || c == ':' || c == ' ' || c == '\t');
}

/// 统计一行管道文本的列数（去掉首尾 `|` 后按 `|` 切分）。
int _pipeCols(String line) {
  var s = line.trim();
  if (s.startsWith('|')) s = s.substring(1);
  if (s.endsWith('|')) s = s.substring(0, s.length - 1);
  return s.split('|').length;
}
