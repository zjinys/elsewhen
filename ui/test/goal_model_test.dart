import 'package:elsewhen_ui/bridge/api.dart' as api;
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
      expect(GoalStatus.paused.wire, 'paused');
      expect(GoalStatus.paused.label, '已暂停');
      expect(GoalStatus.completed.wire, 'completed');
      expect(GoalStatus.completed.label, '已达成');
    });

    test('解析与兼容：superseded 映射到 paused，未知值回落到 active', () {
      expect(GoalStatus.fromString('paused'), GoalStatus.paused);
      expect(GoalStatus.fromString('superseded'), GoalStatus.paused);
      expect(GoalStatus.fromString('completed'), GoalStatus.completed);
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
      expect(goal.isPaused, isFalse);
      expect(goal.isCompleted, isFalse);
      expect(goal.supersededAt, isNull);
    });

    test('转换目标背景字段', () {
      final goal = Goal.fromDto(
        api.GoalDto(
          id: 'g-facet',
          content: '全功能交付',
          phase: 'near',
          status: 'active',
          why: '满足第一批核心用户',
          currentFocus: '完成目标阶段化',
          blocker: 'UI组件待改造',
          doneCriteria: '测试全绿',
          createdAt: '2026-09-28T10:00:00+00:00',
          updatedAt: '2026-09-28T10:00:00+00:00',
        ),
      );
      expect(goal.why, '满足第一批核心用户');
      expect(goal.currentFocus, '完成目标阶段化');
      expect(goal.blocker, 'UI组件待改造');
      expect(goal.doneCriteria, '测试全绿');
    });

    test('已归档目标带归档时间与暂停/达成状态区分', () {
      final pausedGoal = Goal.fromDto(
        _dto(
          id: 'g2',
          content: '旧目标',
          phase: 'long',
          status: 'superseded',
          supersededAt: '2026-09-20T08:00:00+00:00',
        ),
      );
      expect(pausedGoal.isArchived, isTrue);
      expect(pausedGoal.isPaused, isTrue);
      expect(pausedGoal.isCompleted, isFalse);
      expect(pausedGoal.supersededAt, isNotNull);
      // 归档时间需已转本地时区供界面直接格式化
      expect(pausedGoal.supersededAt!.isUtc, isFalse);

      final completedGoal = Goal.fromDto(
        _dto(
          id: 'g3',
          content: '已达成目标',
          phase: 'near',
          status: 'completed',
          supersededAt: '2026-09-25T08:00:00+00:00',
        ),
      );
      expect(completedGoal.isArchived, isTrue);
      expect(completedGoal.isPaused, isFalse);
      expect(completedGoal.isCompleted, isTrue);
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
