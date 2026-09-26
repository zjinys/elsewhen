import '../models/event.dart';
import '../models/analysis.dart';
import '../bridge/generated.dart/api.dart' as api;

/// 存储仓库抽象接口
/// 定义所有存储操作，方便未来切换实现
abstract class StorageRepository {
  /// 初始化存储
  Future<void> initialize();

  /// 记录新事件
  Future<Event> recordEvent(String rawText);

  /// Unified capture path. Implementations without input-record support fall
  /// back to the legacy event API, keeping tests and adapters compatible.
  Future<void> recordUnifiedInput(
    String rawText, {
    String source = 'capture',
  }) async {
    await recordEvent(rawText);
  }

  /// 列出所有事件
  Future<List<Event>> listEvents();

  /// 列出分析结果
  Future<List<Analysis>> listAnalyses();

  /// 获取 AI 提供商信息
  Future<String?> getAiProvider();

  /// 触发 AI 分析
  /// 返回结构化结果：NoProvider 表示尚未配置 provider（不做分析）；
  /// Processed 携带本轮处理条数（0 表示队列已空或全部等待重试）。
  Future<api.AnalysisTriggerResult> triggerAnalysis();
}
