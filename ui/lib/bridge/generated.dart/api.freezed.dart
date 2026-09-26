// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'api.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$AnalysisTriggerResult {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is AnalysisTriggerResult);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'AnalysisTriggerResult()';
}


}

/// @nodoc
class $AnalysisTriggerResultCopyWith<$Res>  {
$AnalysisTriggerResultCopyWith(AnalysisTriggerResult _, $Res Function(AnalysisTriggerResult) __);
}


/// Adds pattern-matching-related methods to [AnalysisTriggerResult].
extension AnalysisTriggerResultPatterns on AnalysisTriggerResult {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( AnalysisTriggerResult_NoProvider value)?  noProvider,TResult Function( AnalysisTriggerResult_Processed value)?  processed,required TResult orElse(),}){
final _that = this;
switch (_that) {
case AnalysisTriggerResult_NoProvider() when noProvider != null:
return noProvider(_that);case AnalysisTriggerResult_Processed() when processed != null:
return processed(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( AnalysisTriggerResult_NoProvider value)  noProvider,required TResult Function( AnalysisTriggerResult_Processed value)  processed,}){
final _that = this;
switch (_that) {
case AnalysisTriggerResult_NoProvider():
return noProvider(_that);case AnalysisTriggerResult_Processed():
return processed(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( AnalysisTriggerResult_NoProvider value)?  noProvider,TResult? Function( AnalysisTriggerResult_Processed value)?  processed,}){
final _that = this;
switch (_that) {
case AnalysisTriggerResult_NoProvider() when noProvider != null:
return noProvider(_that);case AnalysisTriggerResult_Processed() when processed != null:
return processed(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  noProvider,TResult Function( PlatformInt64 count)?  processed,required TResult orElse(),}) {final _that = this;
switch (_that) {
case AnalysisTriggerResult_NoProvider() when noProvider != null:
return noProvider();case AnalysisTriggerResult_Processed() when processed != null:
return processed(_that.count);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  noProvider,required TResult Function( PlatformInt64 count)  processed,}) {final _that = this;
switch (_that) {
case AnalysisTriggerResult_NoProvider():
return noProvider();case AnalysisTriggerResult_Processed():
return processed(_that.count);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  noProvider,TResult? Function( PlatformInt64 count)?  processed,}) {final _that = this;
switch (_that) {
case AnalysisTriggerResult_NoProvider() when noProvider != null:
return noProvider();case AnalysisTriggerResult_Processed() when processed != null:
return processed(_that.count);case _:
  return null;

}
}

}

/// @nodoc


class AnalysisTriggerResult_NoProvider extends AnalysisTriggerResult {
  const AnalysisTriggerResult_NoProvider(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is AnalysisTriggerResult_NoProvider);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'AnalysisTriggerResult.noProvider()';
}


}




/// @nodoc


class AnalysisTriggerResult_Processed extends AnalysisTriggerResult {
  const AnalysisTriggerResult_Processed({required this.count}): super._();
  

 final  PlatformInt64 count;

/// Create a copy of AnalysisTriggerResult
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AnalysisTriggerResult_ProcessedCopyWith<AnalysisTriggerResult_Processed> get copyWith => _$AnalysisTriggerResult_ProcessedCopyWithImpl<AnalysisTriggerResult_Processed>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is AnalysisTriggerResult_Processed&&(identical(other.count, count) || other.count == count));
}


@override
int get hashCode {
    return Object.hash(runtimeType,count);
}

@override
String toString() {
    return 'AnalysisTriggerResult.processed(count: $count)';
}


}

/// @nodoc
abstract mixin class $AnalysisTriggerResult_ProcessedCopyWith<$Res> implements $AnalysisTriggerResultCopyWith<$Res> {
  factory $AnalysisTriggerResult_ProcessedCopyWith(AnalysisTriggerResult_Processed value, $Res Function(AnalysisTriggerResult_Processed) _then) = _$AnalysisTriggerResult_ProcessedCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 count
});




}
/// @nodoc
class _$AnalysisTriggerResult_ProcessedCopyWithImpl<$Res>
    implements $AnalysisTriggerResult_ProcessedCopyWith<$Res> {
  _$AnalysisTriggerResult_ProcessedCopyWithImpl(this._self, this._then);

  final AnalysisTriggerResult_Processed _self;
  final $Res Function(AnalysisTriggerResult_Processed) _then;

/// Create a copy of AnalysisTriggerResult
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? count = null,}) {
  return _then(AnalysisTriggerResult_Processed(
count: null == count ? _self.count : count // ignore: cast_nullable_to_non_nullable
as PlatformInt64,
  ));
}


}

/// @nodoc
mixin _$DailyReviewResult {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is DailyReviewResult);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'DailyReviewResult()';
}


}

/// @nodoc
class $DailyReviewResultCopyWith<$Res>  {
$DailyReviewResultCopyWith(DailyReviewResult _, $Res Function(DailyReviewResult) __);
}


/// Adds pattern-matching-related methods to [DailyReviewResult].
extension DailyReviewResultPatterns on DailyReviewResult {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( DailyReviewResult_NoProvider value)?  noProvider,TResult Function( DailyReviewResult_NoEntries value)?  noEntries,TResult Function( DailyReviewResult_Created value)?  created,required TResult orElse(),}){
final _that = this;
switch (_that) {
case DailyReviewResult_NoProvider() when noProvider != null:
return noProvider(_that);case DailyReviewResult_NoEntries() when noEntries != null:
return noEntries(_that);case DailyReviewResult_Created() when created != null:
return created(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( DailyReviewResult_NoProvider value)  noProvider,required TResult Function( DailyReviewResult_NoEntries value)  noEntries,required TResult Function( DailyReviewResult_Created value)  created,}){
final _that = this;
switch (_that) {
case DailyReviewResult_NoProvider():
return noProvider(_that);case DailyReviewResult_NoEntries():
return noEntries(_that);case DailyReviewResult_Created():
return created(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( DailyReviewResult_NoProvider value)?  noProvider,TResult? Function( DailyReviewResult_NoEntries value)?  noEntries,TResult? Function( DailyReviewResult_Created value)?  created,}){
final _that = this;
switch (_that) {
case DailyReviewResult_NoProvider() when noProvider != null:
return noProvider(_that);case DailyReviewResult_NoEntries() when noEntries != null:
return noEntries(_that);case DailyReviewResult_Created() when created != null:
return created(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  noProvider,TResult Function()?  noEntries,TResult Function( String id)?  created,required TResult orElse(),}) {final _that = this;
switch (_that) {
case DailyReviewResult_NoProvider() when noProvider != null:
return noProvider();case DailyReviewResult_NoEntries() when noEntries != null:
return noEntries();case DailyReviewResult_Created() when created != null:
return created(_that.id);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  noProvider,required TResult Function()  noEntries,required TResult Function( String id)  created,}) {final _that = this;
switch (_that) {
case DailyReviewResult_NoProvider():
return noProvider();case DailyReviewResult_NoEntries():
return noEntries();case DailyReviewResult_Created():
return created(_that.id);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  noProvider,TResult? Function()?  noEntries,TResult? Function( String id)?  created,}) {final _that = this;
switch (_that) {
case DailyReviewResult_NoProvider() when noProvider != null:
return noProvider();case DailyReviewResult_NoEntries() when noEntries != null:
return noEntries();case DailyReviewResult_Created() when created != null:
return created(_that.id);case _:
  return null;

}
}

}

/// @nodoc


class DailyReviewResult_NoProvider extends DailyReviewResult {
  const DailyReviewResult_NoProvider(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is DailyReviewResult_NoProvider);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'DailyReviewResult.noProvider()';
}


}




/// @nodoc


class DailyReviewResult_NoEntries extends DailyReviewResult {
  const DailyReviewResult_NoEntries(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is DailyReviewResult_NoEntries);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'DailyReviewResult.noEntries()';
}


}




/// @nodoc


class DailyReviewResult_Created extends DailyReviewResult {
  const DailyReviewResult_Created({required this.id}): super._();
  

 final  String id;

/// Create a copy of DailyReviewResult
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$DailyReviewResult_CreatedCopyWith<DailyReviewResult_Created> get copyWith => _$DailyReviewResult_CreatedCopyWithImpl<DailyReviewResult_Created>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is DailyReviewResult_Created&&(identical(other.id, id) || other.id == id));
}


@override
int get hashCode {
    return Object.hash(runtimeType,id);
}

@override
String toString() {
    return 'DailyReviewResult.created(id: $id)';
}


}

/// @nodoc
abstract mixin class $DailyReviewResult_CreatedCopyWith<$Res> implements $DailyReviewResultCopyWith<$Res> {
  factory $DailyReviewResult_CreatedCopyWith(DailyReviewResult_Created value, $Res Function(DailyReviewResult_Created) _then) = _$DailyReviewResult_CreatedCopyWithImpl;
@useResult
$Res call({
 String id
});




}
/// @nodoc
class _$DailyReviewResult_CreatedCopyWithImpl<$Res>
    implements $DailyReviewResult_CreatedCopyWith<$Res> {
  _$DailyReviewResult_CreatedCopyWithImpl(this._self, this._then);

  final DailyReviewResult_Created _self;
  final $Res Function(DailyReviewResult_Created) _then;

/// Create a copy of DailyReviewResult
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? id = null,}) {
  return _then(DailyReviewResult_Created(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
