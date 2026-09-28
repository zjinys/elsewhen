// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'knowledge_digest.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$KnowledgeDigestTickResult {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is KnowledgeDigestTickResult);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'KnowledgeDigestTickResult()';
}


}

/// @nodoc
class $KnowledgeDigestTickResultCopyWith<$Res>  {
$KnowledgeDigestTickResultCopyWith(KnowledgeDigestTickResult _, $Res Function(KnowledgeDigestTickResult) __);
}


/// Adds pattern-matching-related methods to [KnowledgeDigestTickResult].
extension KnowledgeDigestTickResultPatterns on KnowledgeDigestTickResult {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( KnowledgeDigestTickResult_NoProvider value)?  noProvider,TResult Function( KnowledgeDigestTickResult_Idle value)?  idle,TResult Function( KnowledgeDigestTickResult_Processed value)?  processed,TResult Function( KnowledgeDigestTickResult_Failed value)?  failed,required TResult orElse(),}){
final _that = this;
switch (_that) {
case KnowledgeDigestTickResult_NoProvider() when noProvider != null:
return noProvider(_that);case KnowledgeDigestTickResult_Idle() when idle != null:
return idle(_that);case KnowledgeDigestTickResult_Processed() when processed != null:
return processed(_that);case KnowledgeDigestTickResult_Failed() when failed != null:
return failed(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( KnowledgeDigestTickResult_NoProvider value)  noProvider,required TResult Function( KnowledgeDigestTickResult_Idle value)  idle,required TResult Function( KnowledgeDigestTickResult_Processed value)  processed,required TResult Function( KnowledgeDigestTickResult_Failed value)  failed,}){
final _that = this;
switch (_that) {
case KnowledgeDigestTickResult_NoProvider():
return noProvider(_that);case KnowledgeDigestTickResult_Idle():
return idle(_that);case KnowledgeDigestTickResult_Processed():
return processed(_that);case KnowledgeDigestTickResult_Failed():
return failed(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( KnowledgeDigestTickResult_NoProvider value)?  noProvider,TResult? Function( KnowledgeDigestTickResult_Idle value)?  idle,TResult? Function( KnowledgeDigestTickResult_Processed value)?  processed,TResult? Function( KnowledgeDigestTickResult_Failed value)?  failed,}){
final _that = this;
switch (_that) {
case KnowledgeDigestTickResult_NoProvider() when noProvider != null:
return noProvider(_that);case KnowledgeDigestTickResult_Idle() when idle != null:
return idle(_that);case KnowledgeDigestTickResult_Processed() when processed != null:
return processed(_that);case KnowledgeDigestTickResult_Failed() when failed != null:
return failed(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  noProvider,TResult Function()?  idle,TResult Function( PlatformInt64 events,  List<String> createdSlugs,  List<String> updatedSlugs,  List<String> protectedSlugs)?  processed,TResult Function( PlatformInt64 events,  String error)?  failed,required TResult orElse(),}) {final _that = this;
switch (_that) {
case KnowledgeDigestTickResult_NoProvider() when noProvider != null:
return noProvider();case KnowledgeDigestTickResult_Idle() when idle != null:
return idle();case KnowledgeDigestTickResult_Processed() when processed != null:
return processed(_that.events,_that.createdSlugs,_that.updatedSlugs,_that.protectedSlugs);case KnowledgeDigestTickResult_Failed() when failed != null:
return failed(_that.events,_that.error);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  noProvider,required TResult Function()  idle,required TResult Function( PlatformInt64 events,  List<String> createdSlugs,  List<String> updatedSlugs,  List<String> protectedSlugs)  processed,required TResult Function( PlatformInt64 events,  String error)  failed,}) {final _that = this;
switch (_that) {
case KnowledgeDigestTickResult_NoProvider():
return noProvider();case KnowledgeDigestTickResult_Idle():
return idle();case KnowledgeDigestTickResult_Processed():
return processed(_that.events,_that.createdSlugs,_that.updatedSlugs,_that.protectedSlugs);case KnowledgeDigestTickResult_Failed():
return failed(_that.events,_that.error);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  noProvider,TResult? Function()?  idle,TResult? Function( PlatformInt64 events,  List<String> createdSlugs,  List<String> updatedSlugs,  List<String> protectedSlugs)?  processed,TResult? Function( PlatformInt64 events,  String error)?  failed,}) {final _that = this;
switch (_that) {
case KnowledgeDigestTickResult_NoProvider() when noProvider != null:
return noProvider();case KnowledgeDigestTickResult_Idle() when idle != null:
return idle();case KnowledgeDigestTickResult_Processed() when processed != null:
return processed(_that.events,_that.createdSlugs,_that.updatedSlugs,_that.protectedSlugs);case KnowledgeDigestTickResult_Failed() when failed != null:
return failed(_that.events,_that.error);case _:
  return null;

}
}

}

/// @nodoc


class KnowledgeDigestTickResult_NoProvider extends KnowledgeDigestTickResult {
  const KnowledgeDigestTickResult_NoProvider(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is KnowledgeDigestTickResult_NoProvider);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'KnowledgeDigestTickResult.noProvider()';
}


}




/// @nodoc


class KnowledgeDigestTickResult_Idle extends KnowledgeDigestTickResult {
  const KnowledgeDigestTickResult_Idle(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is KnowledgeDigestTickResult_Idle);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'KnowledgeDigestTickResult.idle()';
}


}




/// @nodoc


class KnowledgeDigestTickResult_Processed extends KnowledgeDigestTickResult {
  const KnowledgeDigestTickResult_Processed({required this.events, required  List<String> createdSlugs, required  List<String> updatedSlugs, required  List<String> protectedSlugs}): _createdSlugs = createdSlugs,_updatedSlugs = updatedSlugs,_protectedSlugs = protectedSlugs,super._();
  

 final  PlatformInt64 events;
 final  List<String> _createdSlugs;
 List<String> get createdSlugs {
  if (_createdSlugs is EqualUnmodifiableListView) return _createdSlugs;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_createdSlugs);
}

 final  List<String> _updatedSlugs;
 List<String> get updatedSlugs {
  if (_updatedSlugs is EqualUnmodifiableListView) return _updatedSlugs;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_updatedSlugs);
}

 final  List<String> _protectedSlugs;
 List<String> get protectedSlugs {
  if (_protectedSlugs is EqualUnmodifiableListView) return _protectedSlugs;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_protectedSlugs);
}


/// Create a copy of KnowledgeDigestTickResult
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$KnowledgeDigestTickResult_ProcessedCopyWith<KnowledgeDigestTickResult_Processed> get copyWith => _$KnowledgeDigestTickResult_ProcessedCopyWithImpl<KnowledgeDigestTickResult_Processed>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is KnowledgeDigestTickResult_Processed&&(identical(other.events, events) || other.events == events)&&const DeepCollectionEquality().equals(other.createdSlugs, _createdSlugs)&&const DeepCollectionEquality().equals(other.updatedSlugs, _updatedSlugs)&&const DeepCollectionEquality().equals(other.protectedSlugs, _protectedSlugs));
}


@override
int get hashCode {
    return Object.hash(runtimeType,events,const DeepCollectionEquality().hash(_createdSlugs),const DeepCollectionEquality().hash(_updatedSlugs),const DeepCollectionEquality().hash(_protectedSlugs));
}

@override
String toString() {
    return 'KnowledgeDigestTickResult.processed(events: $events, createdSlugs: $createdSlugs, updatedSlugs: $updatedSlugs, protectedSlugs: $protectedSlugs)';
}


}

/// @nodoc
abstract mixin class $KnowledgeDigestTickResult_ProcessedCopyWith<$Res> implements $KnowledgeDigestTickResultCopyWith<$Res> {
  factory $KnowledgeDigestTickResult_ProcessedCopyWith(KnowledgeDigestTickResult_Processed value, $Res Function(KnowledgeDigestTickResult_Processed) _then) = _$KnowledgeDigestTickResult_ProcessedCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 events, List<String> createdSlugs, List<String> updatedSlugs, List<String> protectedSlugs
});




}
/// @nodoc
class _$KnowledgeDigestTickResult_ProcessedCopyWithImpl<$Res>
    implements $KnowledgeDigestTickResult_ProcessedCopyWith<$Res> {
  _$KnowledgeDigestTickResult_ProcessedCopyWithImpl(this._self, this._then);

  final KnowledgeDigestTickResult_Processed _self;
  final $Res Function(KnowledgeDigestTickResult_Processed) _then;

/// Create a copy of KnowledgeDigestTickResult
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? events = null,Object? createdSlugs = null,Object? updatedSlugs = null,Object? protectedSlugs = null,}) {
  return _then(KnowledgeDigestTickResult_Processed(
events: null == events ? _self.events : events // ignore: cast_nullable_to_non_nullable
as PlatformInt64,createdSlugs: null == createdSlugs ? _self._createdSlugs : createdSlugs // ignore: cast_nullable_to_non_nullable
as List<String>,updatedSlugs: null == updatedSlugs ? _self._updatedSlugs : updatedSlugs // ignore: cast_nullable_to_non_nullable
as List<String>,protectedSlugs: null == protectedSlugs ? _self._protectedSlugs : protectedSlugs // ignore: cast_nullable_to_non_nullable
as List<String>,
  ));
}


}

/// @nodoc


class KnowledgeDigestTickResult_Failed extends KnowledgeDigestTickResult {
  const KnowledgeDigestTickResult_Failed({required this.events, required this.error}): super._();
  

 final  PlatformInt64 events;
 final  String error;

/// Create a copy of KnowledgeDigestTickResult
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$KnowledgeDigestTickResult_FailedCopyWith<KnowledgeDigestTickResult_Failed> get copyWith => _$KnowledgeDigestTickResult_FailedCopyWithImpl<KnowledgeDigestTickResult_Failed>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is KnowledgeDigestTickResult_Failed&&(identical(other.events, events) || other.events == events)&&(identical(other.error, error) || other.error == error));
}


@override
int get hashCode {
    return Object.hash(runtimeType,events,error);
}

@override
String toString() {
    return 'KnowledgeDigestTickResult.failed(events: $events, error: $error)';
}


}

/// @nodoc
abstract mixin class $KnowledgeDigestTickResult_FailedCopyWith<$Res> implements $KnowledgeDigestTickResultCopyWith<$Res> {
  factory $KnowledgeDigestTickResult_FailedCopyWith(KnowledgeDigestTickResult_Failed value, $Res Function(KnowledgeDigestTickResult_Failed) _then) = _$KnowledgeDigestTickResult_FailedCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 events, String error
});




}
/// @nodoc
class _$KnowledgeDigestTickResult_FailedCopyWithImpl<$Res>
    implements $KnowledgeDigestTickResult_FailedCopyWith<$Res> {
  _$KnowledgeDigestTickResult_FailedCopyWithImpl(this._self, this._then);

  final KnowledgeDigestTickResult_Failed _self;
  final $Res Function(KnowledgeDigestTickResult_Failed) _then;

/// Create a copy of KnowledgeDigestTickResult
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? events = null,Object? error = null,}) {
  return _then(KnowledgeDigestTickResult_Failed(
events: null == events ? _self.events : events // ignore: cast_nullable_to_non_nullable
as PlatformInt64,error: null == error ? _self.error : error // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
