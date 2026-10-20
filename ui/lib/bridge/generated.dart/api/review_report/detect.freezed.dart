// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'detect.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$PeriodReportTickResult {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is PeriodReportTickResult);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'PeriodReportTickResult()';
}


}

/// @nodoc
class $PeriodReportTickResultCopyWith<$Res>  {
$PeriodReportTickResultCopyWith(PeriodReportTickResult _, $Res Function(PeriodReportTickResult) __);
}


/// Adds pattern-matching-related methods to [PeriodReportTickResult].
extension PeriodReportTickResultPatterns on PeriodReportTickResult {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( PeriodReportTickResult_NoProvider value)?  noProvider,TResult Function( PeriodReportTickResult_Idle value)?  idle,TResult Function( PeriodReportTickResult_Generated value)?  generated,TResult Function( PeriodReportTickResult_Detected value)?  detected,TResult Function( PeriodReportTickResult_DetectFailed value)?  detectFailed,required TResult orElse(),}){
final _that = this;
switch (_that) {
case PeriodReportTickResult_NoProvider() when noProvider != null:
return noProvider(_that);case PeriodReportTickResult_Idle() when idle != null:
return idle(_that);case PeriodReportTickResult_Generated() when generated != null:
return generated(_that);case PeriodReportTickResult_Detected() when detected != null:
return detected(_that);case PeriodReportTickResult_DetectFailed() when detectFailed != null:
return detectFailed(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( PeriodReportTickResult_NoProvider value)  noProvider,required TResult Function( PeriodReportTickResult_Idle value)  idle,required TResult Function( PeriodReportTickResult_Generated value)  generated,required TResult Function( PeriodReportTickResult_Detected value)  detected,required TResult Function( PeriodReportTickResult_DetectFailed value)  detectFailed,}){
final _that = this;
switch (_that) {
case PeriodReportTickResult_NoProvider():
return noProvider(_that);case PeriodReportTickResult_Idle():
return idle(_that);case PeriodReportTickResult_Generated():
return generated(_that);case PeriodReportTickResult_Detected():
return detected(_that);case PeriodReportTickResult_DetectFailed():
return detectFailed(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( PeriodReportTickResult_NoProvider value)?  noProvider,TResult? Function( PeriodReportTickResult_Idle value)?  idle,TResult? Function( PeriodReportTickResult_Generated value)?  generated,TResult? Function( PeriodReportTickResult_Detected value)?  detected,TResult? Function( PeriodReportTickResult_DetectFailed value)?  detectFailed,}){
final _that = this;
switch (_that) {
case PeriodReportTickResult_NoProvider() when noProvider != null:
return noProvider(_that);case PeriodReportTickResult_Idle() when idle != null:
return idle(_that);case PeriodReportTickResult_Generated() when generated != null:
return generated(_that);case PeriodReportTickResult_Detected() when detected != null:
return detected(_that);case PeriodReportTickResult_DetectFailed() when detectFailed != null:
return detectFailed(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  noProvider,TResult Function()?  idle,TResult Function( String period,  String periodType)?  generated,TResult Function( String reportId,  BigInt deviations)?  detected,TResult Function( String reportId,  String error)?  detectFailed,required TResult orElse(),}) {final _that = this;
switch (_that) {
case PeriodReportTickResult_NoProvider() when noProvider != null:
return noProvider();case PeriodReportTickResult_Idle() when idle != null:
return idle();case PeriodReportTickResult_Generated() when generated != null:
return generated(_that.period,_that.periodType);case PeriodReportTickResult_Detected() when detected != null:
return detected(_that.reportId,_that.deviations);case PeriodReportTickResult_DetectFailed() when detectFailed != null:
return detectFailed(_that.reportId,_that.error);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  noProvider,required TResult Function()  idle,required TResult Function( String period,  String periodType)  generated,required TResult Function( String reportId,  BigInt deviations)  detected,required TResult Function( String reportId,  String error)  detectFailed,}) {final _that = this;
switch (_that) {
case PeriodReportTickResult_NoProvider():
return noProvider();case PeriodReportTickResult_Idle():
return idle();case PeriodReportTickResult_Generated():
return generated(_that.period,_that.periodType);case PeriodReportTickResult_Detected():
return detected(_that.reportId,_that.deviations);case PeriodReportTickResult_DetectFailed():
return detectFailed(_that.reportId,_that.error);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  noProvider,TResult? Function()?  idle,TResult? Function( String period,  String periodType)?  generated,TResult? Function( String reportId,  BigInt deviations)?  detected,TResult? Function( String reportId,  String error)?  detectFailed,}) {final _that = this;
switch (_that) {
case PeriodReportTickResult_NoProvider() when noProvider != null:
return noProvider();case PeriodReportTickResult_Idle() when idle != null:
return idle();case PeriodReportTickResult_Generated() when generated != null:
return generated(_that.period,_that.periodType);case PeriodReportTickResult_Detected() when detected != null:
return detected(_that.reportId,_that.deviations);case PeriodReportTickResult_DetectFailed() when detectFailed != null:
return detectFailed(_that.reportId,_that.error);case _:
  return null;

}
}

}

/// @nodoc


class PeriodReportTickResult_NoProvider extends PeriodReportTickResult {
  const PeriodReportTickResult_NoProvider(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is PeriodReportTickResult_NoProvider);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'PeriodReportTickResult.noProvider()';
}


}




/// @nodoc


class PeriodReportTickResult_Idle extends PeriodReportTickResult {
  const PeriodReportTickResult_Idle(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is PeriodReportTickResult_Idle);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'PeriodReportTickResult.idle()';
}


}




/// @nodoc


class PeriodReportTickResult_Generated extends PeriodReportTickResult {
  const PeriodReportTickResult_Generated({required this.period, required this.periodType}): super._();
  

 final  String period;
 final  String periodType;

/// Create a copy of PeriodReportTickResult
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$PeriodReportTickResult_GeneratedCopyWith<PeriodReportTickResult_Generated> get copyWith => _$PeriodReportTickResult_GeneratedCopyWithImpl<PeriodReportTickResult_Generated>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is PeriodReportTickResult_Generated&&(identical(other.period, period) || other.period == period)&&(identical(other.periodType, periodType) || other.periodType == periodType));
}


@override
int get hashCode {
    return Object.hash(runtimeType,period,periodType);
}

@override
String toString() {
    return 'PeriodReportTickResult.generated(period: $period, periodType: $periodType)';
}


}

/// @nodoc
abstract mixin class $PeriodReportTickResult_GeneratedCopyWith<$Res> implements $PeriodReportTickResultCopyWith<$Res> {
  factory $PeriodReportTickResult_GeneratedCopyWith(PeriodReportTickResult_Generated value, $Res Function(PeriodReportTickResult_Generated) _then) = _$PeriodReportTickResult_GeneratedCopyWithImpl;
@useResult
$Res call({
 String period, String periodType
});




}
/// @nodoc
class _$PeriodReportTickResult_GeneratedCopyWithImpl<$Res>
    implements $PeriodReportTickResult_GeneratedCopyWith<$Res> {
  _$PeriodReportTickResult_GeneratedCopyWithImpl(this._self, this._then);

  final PeriodReportTickResult_Generated _self;
  final $Res Function(PeriodReportTickResult_Generated) _then;

/// Create a copy of PeriodReportTickResult
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? period = null,Object? periodType = null,}) {
  return _then(PeriodReportTickResult_Generated(
period: null == period ? _self.period : period // ignore: cast_nullable_to_non_nullable
as String,periodType: null == periodType ? _self.periodType : periodType // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class PeriodReportTickResult_Detected extends PeriodReportTickResult {
  const PeriodReportTickResult_Detected({required this.reportId, required this.deviations}): super._();
  

 final  String reportId;
 final  BigInt deviations;

/// Create a copy of PeriodReportTickResult
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$PeriodReportTickResult_DetectedCopyWith<PeriodReportTickResult_Detected> get copyWith => _$PeriodReportTickResult_DetectedCopyWithImpl<PeriodReportTickResult_Detected>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is PeriodReportTickResult_Detected&&(identical(other.reportId, reportId) || other.reportId == reportId)&&(identical(other.deviations, deviations) || other.deviations == deviations));
}


@override
int get hashCode {
    return Object.hash(runtimeType,reportId,deviations);
}

@override
String toString() {
    return 'PeriodReportTickResult.detected(reportId: $reportId, deviations: $deviations)';
}


}

/// @nodoc
abstract mixin class $PeriodReportTickResult_DetectedCopyWith<$Res> implements $PeriodReportTickResultCopyWith<$Res> {
  factory $PeriodReportTickResult_DetectedCopyWith(PeriodReportTickResult_Detected value, $Res Function(PeriodReportTickResult_Detected) _then) = _$PeriodReportTickResult_DetectedCopyWithImpl;
@useResult
$Res call({
 String reportId, BigInt deviations
});




}
/// @nodoc
class _$PeriodReportTickResult_DetectedCopyWithImpl<$Res>
    implements $PeriodReportTickResult_DetectedCopyWith<$Res> {
  _$PeriodReportTickResult_DetectedCopyWithImpl(this._self, this._then);

  final PeriodReportTickResult_Detected _self;
  final $Res Function(PeriodReportTickResult_Detected) _then;

/// Create a copy of PeriodReportTickResult
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? reportId = null,Object? deviations = null,}) {
  return _then(PeriodReportTickResult_Detected(
reportId: null == reportId ? _self.reportId : reportId // ignore: cast_nullable_to_non_nullable
as String,deviations: null == deviations ? _self.deviations : deviations // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}


}

/// @nodoc


class PeriodReportTickResult_DetectFailed extends PeriodReportTickResult {
  const PeriodReportTickResult_DetectFailed({required this.reportId, required this.error}): super._();
  

 final  String reportId;
 final  String error;

/// Create a copy of PeriodReportTickResult
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$PeriodReportTickResult_DetectFailedCopyWith<PeriodReportTickResult_DetectFailed> get copyWith => _$PeriodReportTickResult_DetectFailedCopyWithImpl<PeriodReportTickResult_DetectFailed>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is PeriodReportTickResult_DetectFailed&&(identical(other.reportId, reportId) || other.reportId == reportId)&&(identical(other.error, error) || other.error == error));
}


@override
int get hashCode {
    return Object.hash(runtimeType,reportId,error);
}

@override
String toString() {
    return 'PeriodReportTickResult.detectFailed(reportId: $reportId, error: $error)';
}


}

/// @nodoc
abstract mixin class $PeriodReportTickResult_DetectFailedCopyWith<$Res> implements $PeriodReportTickResultCopyWith<$Res> {
  factory $PeriodReportTickResult_DetectFailedCopyWith(PeriodReportTickResult_DetectFailed value, $Res Function(PeriodReportTickResult_DetectFailed) _then) = _$PeriodReportTickResult_DetectFailedCopyWithImpl;
@useResult
$Res call({
 String reportId, String error
});




}
/// @nodoc
class _$PeriodReportTickResult_DetectFailedCopyWithImpl<$Res>
    implements $PeriodReportTickResult_DetectFailedCopyWith<$Res> {
  _$PeriodReportTickResult_DetectFailedCopyWithImpl(this._self, this._then);

  final PeriodReportTickResult_DetectFailed _self;
  final $Res Function(PeriodReportTickResult_DetectFailed) _then;

/// Create a copy of PeriodReportTickResult
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? reportId = null,Object? error = null,}) {
  return _then(PeriodReportTickResult_DetectFailed(
reportId: null == reportId ? _self.reportId : reportId // ignore: cast_nullable_to_non_nullable
as String,error: null == error ? _self.error : error // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
