import 'package:flutter/foundation.dart';

import '../bridge/api.dart' as api;

/// 加工移交（P4d）的 FRB 入口（可注入测试替身）。
///
/// 对应 `src/api/sync.rs::get_processor_handoff_state` /
/// `yield_sync_processor` / `approve_processor_handoff` /
/// `confirm_processor_takeover`。所有写操作在 Rust 侧失败关闭：静默未达成、
/// frontier 未追平、非 owner 审批都会抛出同步错误码。
abstract interface class ProcessorHandoffApi {
  Future<api.ProcessorHandoffStateDto> getState();

  Future<api.ProcessingYieldProofDto> yieldProcessor({
    required String toDeviceId,
  });

  Future<void> approve({required api.ProcessingYieldProofDto proof});

  Future<void> confirmTakeover();
}

class FrbProcessorHandoffApi implements ProcessorHandoffApi {
  const FrbProcessorHandoffApi();

  @override
  Future<api.ProcessorHandoffStateDto> getState() =>
      api.getProcessorHandoffState();

  @override
  Future<api.ProcessingYieldProofDto> yieldProcessor({
    required String toDeviceId,
  }) => api.yieldSyncProcessor(toDeviceId: toDeviceId);

  @override
  Future<void> approve({required api.ProcessingYieldProofDto proof}) =>
      api.approveProcessorHandoff(proof: proof);

  @override
  Future<void> confirmTakeover() => api.confirmProcessorTakeover();
}

/// 设置页的加工移交状态与显式动作。
///
/// 只展示状态与两个显式按钮（让出 / 接管），绝不自动推进任何状态机步骤；
/// 让出后会尝试 owner 审批（仅当本机是 owner），其余等待目标设备自行确认。
/// 失败保留旧视图并显示错误，绝不静默清空。
class SyncProcessorHandoffController extends ChangeNotifier {
  SyncProcessorHandoffController({ProcessorHandoffApi? bridge})
    : _bridge = bridge ?? const FrbProcessorHandoffApi();

  final ProcessorHandoffApi _bridge;

  api.ProcessorHandoffStateDto? state;
  String? error;
  bool busy = false;
  bool _disposed = false;

  void _changed() {
    if (!_disposed) notifyListeners();
  }

  Future<void> refresh() async {
    try {
      state = await _bridge.getState();
      error = null;
    } catch (e) {
      error = handoffErrorText(e.toString());
    }
    _changed();
  }

  /// 旧加工端让出加工权。[isOwnerDevice] 为真时（owner 与旧加工端同机），
  /// 生成证明后立即由本机 owner 审批签发后继名册。
  Future<void> yieldTo({
    required String toDeviceId,
    required bool isOwnerDevice,
  }) async {
    if (toDeviceId.trim().isEmpty) {
      error = '请填写目标设备 ID。';
      _changed();
      return;
    }
    busy = true;
    error = null;
    _changed();
    try {
      final proof = await _bridge.yieldProcessor(toDeviceId: toDeviceId.trim());
      if (isOwnerDevice) {
        await _bridge.approve(proof: proof);
      }
      await refresh();
    } catch (e) {
      error = handoffErrorText(e.toString());
    } finally {
      busy = false;
      _changed();
    }
  }

  /// 接管目标确认：仅在 Rust 侧 frontier 已追平时成功。
  Future<void> confirm() async {
    busy = true;
    error = null;
    _changed();
    try {
      await _bridge.confirmTakeover();
      await refresh();
    } catch (e) {
      error = handoffErrorText(e.toString());
    } finally {
      busy = false;
      _changed();
    }
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}

/// 同步错误码 → 设置页可读文案；未识别的原样透传（绝不吃掉细节）。
String handoffErrorText(String raw) {
  // FRB 异常可能带 anyhow 前缀/堆栈，仅做包含匹配。
  String? matched;
  for (final entry in _handoffErrors.entries) {
    if (raw.contains(entry.key)) {
      matched = entry.value;
      break;
    }
  }
  return matched ?? raw;
}

const Map<String, String> _handoffErrors = {
  'sync_membership_not_processor': '本机当前不是加工设备，无需让出。',
  'sync_membership_owner_required': '只有 owner 设备才能审批移交证明。',
  'sync_background_processing_must_finish_before_enable':
      '本机仍有进行中的后台任务，必须等它们完成后才能让出加工权。',
  'sync_frontier_lagging': '本机尚未同步到移交边界，请先等待同步追平再确认接管。',
  'sync_processor_not_in_pending_takeover': '本机没有待确认的加工接管。',
  'sync_handoff_signature_invalid': '移交证明签名无效，可能已被篡改。',
  'sync_handoff_revision_invalid': '移交证明信息超出有效范围。',
  'sync_handoff_revision_mismatch': '移交证明版本与当前名册不一致，请重新在旧加工设备上让出。',
  'sync_handoff_source_mismatch': '移交证明与当前加工设备不符，请重新在旧加工设备上让出。',
  'sync_handoff_target_missing': '移交目标不是当前名册中的活跃设备。',
  'sync_handoff_old_processor_inactive': '旧加工设备已不在名册中，无法审批此证明。',
  'sync_handoff_dataset_mismatch': '移交证明与当前数据集不符。',
  'sync_handoff_self_transfer_invalid': '加工权不能移交给本机自己。',
  'sync_handoff_proof_target_mismatch': '移交证明与本机状态不符。',
  'sync_handoff_proof_required': '缺少移交证明，无法应用此设备变更。',
  'sync_handoff_proof_source_mismatch': '移交证明来源与本机状态不符。',
  'sync_handoff_source_missing': '移交证明缺少来源设备。',
  'sync_handoff_target_mismatch': '移交证明目标与本机不符。',
  'sync_processing_handoff_required': '缺少加工移交证明，本设备无法采纳此变更。',
  'sync_handoff_unexpected_without_processor_change': '名册未变更加工设备却携带移交证明。',
  'sync_runtime_locked': '本机同步密钥已锁定，请先解锁后再操作。',
  'sync_not_initialized': '本机尚未准备同步身份。',
};
