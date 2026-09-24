import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/widgets/markdown_view.dart';

/// UI 边界兜底：工具协议残留不渲染（代码围栏区除外）。
void main() {
  test('strips tool_call block but keeps prose', () {
    const input =
        '关于小生意说的很透彻。\n<tool_call>search_knowledge_base<arg_key>query</arg_key><arg_value>感悟</arg_value></tool_call>';
    final out = stripToolProtocolForDisplay(input);
    expect(out, contains('关于小生意'));
    expect(out, isNot(contains('tool_call')));
    expect(out, isNot(contains('arg_')));
  });

  test('drops residue lines of other dialects', () {
    const input = '正文\n[工具调用]{"name":"x"\n<|invoke name="y">\n尾巴';
    final out = stripToolProtocolForDisplay(input);
    expect(out, contains('正文'));
    expect(out, contains('尾巴'));
    expect(out, isNot(contains('工具调用')));
    expect(out, isNot(contains('invoke')));
  });

  test('keeps fenced code intact', () {
    const input =
        '讨论一下格式：\n```\n<tool_call>search_knowledge_base<arg_key>query</arg_key></tool_call>\n```\n结束';
    final out = stripToolProtocolForDisplay(input);
    expect(out, contains('<tool_call>'));
    expect(out, contains('结束'));
  });

  test('plain text untouched', () {
    const input = '今天天气不错\n适合出门走走';
    expect(stripToolProtocolForDisplay(input), input);
  });
}
