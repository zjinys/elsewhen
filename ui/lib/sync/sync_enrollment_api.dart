import '../bridge/api.dart' as api;

abstract interface class SyncEnrollmentApi {
  Future<List<api.SyncEnrollmentStateDto>> list();
  Future<api.SyncEnrollmentStateDto> advance(api.SyncEnrollmentStateDto state);
  Future<api.SyncEnrollmentStateDto> confirm(
    api.SyncEnrollmentStateDto state,
    int protectedRevision,
  );
  Future<bool> release(api.SyncEnrollmentStateDto state);
}

class FrbSyncEnrollmentApi implements SyncEnrollmentApi {
  @override
  Future<List<api.SyncEnrollmentStateDto>> list() => api.listSyncEnrollments();

  @override
  Future<api.SyncEnrollmentStateDto> advance(
    api.SyncEnrollmentStateDto state,
  ) => api.advanceSyncEnrollment(
    role: state.role,
    operationId: state.operationId,
    invitationId: state.invitationId,
  );

  @override
  Future<api.SyncEnrollmentStateDto> confirm(
    api.SyncEnrollmentStateDto state,
    int protectedRevision,
  ) {
    final token = state.confirmationToken;
    if (token == null) throw StateError('sync_enrollment_confirmation_missing');
    return api.confirmSyncEnrollment(
      role: state.role,
      operationId: state.operationId,
      invitationId: state.invitationId,
      token: token,
      protectedRevision: protectedRevision,
    );
  }

  @override
  Future<bool> release(api.SyncEnrollmentStateDto state) =>
      switch (state.role) {
        'owner' => api.releaseSyncKeyRequest(token: state.operationId),
        'joiner' => api.releaseSyncJoin(operationId: state.operationId),
        _ => throw StateError('sync_enrollment_role_mismatch'),
      };
}
