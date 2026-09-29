import 'package:elsewhen_ui/bridge/generated.dart/api.dart' as api;
import 'package:elsewhen_ui/models/goal.dart';
import 'package:elsewhen_ui/widgets/goal_view.dart';
import 'package:flutter_test/flutter_test.dart';

api.GoalDto _dto({
  required String id,
  required String content,
  required String phase,
  String status = 'active',
  String? supersededAt,
}) {
  return api.GoalDto(
    id: id,
    content: content,
    phase: phase,
    status: status,
    createdAt: '2026-09-28T10:00:00+00:00',
    updatedAt: '2026-09-28T10:00:00+00:00',
    supersededAt: supersededAt,
  );
}

void main() {
  group('GoalPhase', () {
    test('三阶段与后端 wire 值对应', () {
      expect(GoalPhase.near.wire, 'near');
      expect(GoalPhase.mid.wire, 'mid');
      expect(GoalPhase.long.wire, 'long');
    });

    test('中文标签可读', () {
      expect(GoalPhase.near.label, '近期');
      expect(GoalPhase.mid.label, '中期');
      expect(GoalPhase.long.label, '长远');
    });

    test('未知值回落到 near，不抛异常', () {
      expect(GoalPhase.fromString('near'), GoalPhase.near);
      expect(GoalPhase.fromString('mid'), GoalPhase.mid);
      expect(GoalPhase.fromString('long'), GoalPhase.long);
      expect(GoalPhase.fromString('??'), GoalPhase.near);
    });
  });

  group('GoalStatus', () {
    test('wire 值与中文标签', () {
      expect(GoalStatus.active.wire, 'active');
      expect(GoalStatus.active.label, '进行中');
      expect(GoalStatus.superseded.wire, 'superseded');
      expect(GoalStatus.superseded.label, '已归档');
    });

    test('未知值回落到 active', () {
      expect(GoalStatus.fromString('superseded'), GoalStatus.superseded);
      expect(GoalStatus.fromString('active'), GoalStatus.active);
      expect(GoalStatus.fromString('??'), GoalStatus.active);
    });
  });

  group('Goal.fromDto', () {
    test('转换字段与时间戳', () {
      final goal = Goal.fromDto(_dto(id: 'g1', content: '上线 v1', phase: 'mid'));
      expect(goal.id, 'g1');
      expect(goal.content, '上线 v1');
      expect(goal.phase, GoalPhase.mid);
      expect(goal.status, GoalStatus.active);
      expect(goal.isArchived, isFalse);
      expect(goal.supersededAt, isNull);
    });

    test('已归档目标带归档时间', () {
      final goal = Goal.fromDto(
        _dto(
          id: 'g2',
          content: '旧目标',
          phase: 'long',
          status: 'superseded',
          supersededAt: '2026-09-20T08:00:00+00:00',
        ),
      );
      expect(goal.isArchived, isTrue);
      expect(goal.supersededAt, isNotNull);
      // 归档时间需已转本地时区供界面直接格式化
      expect(goal.supersededAt!.isUtc, isFalse);
    });
  });

  group('目标上限常量', () {
    // 界面侧的 maxActiveGoals 只用于提前禁用，后端触发器才是最终防线；
    // 这里断言的是界面真实使用的那份常量，防止它与后端悄悄漂移成不同数字。
    test('界面侧上限为 3，与后端 MAX_ACTIVE_GOALS 一致', () {
      expect(maxActiveGoals, 3);
    });
  });
}
