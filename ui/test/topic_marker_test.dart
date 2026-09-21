import 'package:elsewhen_ui/widgets/message_area.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('recognizes explicit topic markers', () {
    expect(explicitTopicName('#旅行计划'), '旅行计划');
    expect(explicitTopicName('/topic 旅行计划'), '旅行计划');
    expect(explicitTopicName('/TOPIC 旅行计划'), '旅行计划');
    expect(explicitTopicName('进入主题：旅行计划'), '旅行计划');
    expect(explicitTopicName('进入主题: 旅行计划'), '旅行计划');
  });

  test('does not classify ordinary input as a topic marker', () {
    expect(explicitTopicName('今天讨论了 #旅行计划'), isNull);
    expect(explicitTopicName('#'), isNull);
    expect(explicitTopicName('/topic'), isNull);
  });
}
