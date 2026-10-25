#include "uvmcheck/rules/BuiltinRules.h"
#include "uvmcheck/rules/RuleRegistry.h"

namespace uvmcheck::rules {

// UVM rules
std::unique_ptr<Rule> createUvmComponentDirectNew();
std::unique_ptr<Rule> createUvmMissingSuperPhase();
std::unique_ptr<Rule> createUvmTimeInFunctionPhase();
std::unique_ptr<Rule> createUvmConfigDbNullContext();
std::unique_ptr<Rule> createUvmFactoryMacroMismatch();
std::unique_ptr<Rule> createUvmObjectionInLoop();
std::unique_ptr<Rule> createUvmFactoryMissingUtils();
std::unique_ptr<Rule> createUvmPhaseSignatureMismatch();
std::unique_ptr<Rule> createUvmSequenceDirectVifAccess();
std::unique_ptr<Rule> createUvmUnpairedObjection();
std::unique_ptr<Rule> createUvmConfigDbTypeMismatch();
std::unique_ptr<Rule> createUvmUnconnectedTlmPort();
std::unique_ptr<Rule> createUvmSequencePrePostBodyMisuse();
std::unique_ptr<Rule> createUvmPSequencerAbuse();
std::unique_ptr<Rule> createObserverStateMutationInWrite();
std::unique_ptr<Rule> createDirectRalMirrorTampering();

// OOP rules
std::unique_ptr<Rule> createNonVirtualMethodOverride();
std::unique_ptr<Rule> createUncheckedDynamicCast();
std::unique_ptr<Rule> createMissingSuperNew();
std::unique_ptr<Rule> createVariableShadowing();
std::unique_ptr<Rule> createDuplicateMethodBody();
std::unique_ptr<Rule> createVirtualCallInConstructor();
std::unique_ptr<Rule> createClassShallowCopyAliasing();
std::unique_ptr<Rule> createStaticMemberInUvmComponent();

// Concurrency rules
std::unique_ptr<Rule> createForkLoopVariableShared();
std::unique_ptr<Rule> createStaticRoutineVariableLifetime();
std::unique_ptr<Rule> createZeroDelayEventWaitLoop();
std::unique_ptr<Rule> createDisableForkWithoutScope();
std::unique_ptr<Rule> createWaitModifiedInLoop();

// Performance rules
std::unique_ptr<Rule> createPassArrayByValue();
std::unique_ptr<Rule> createArrayReductionWidthTruncation();
std::unique_ptr<Rule> createDynArrayIncrementalResizeInLoop();
std::unique_ptr<Rule> createProceduralZeroDelay();

// DeadCode rules
std::unique_ptr<Rule> createUnusedVariable();
std::unique_ptr<Rule> createUnreachableStatementsPostReturn();
std::unique_ptr<Rule> createAccidentalEmptyConditional();
std::unique_ptr<Rule> createConsecutiveDeadStore();

// Types and Arithmetic rules
std::unique_ptr<Rule> createMixedSignComparison();

// Randomization & Constraints rules
std::unique_ptr<Rule> createUncheckedRandomize();
std::unique_ptr<Rule> createGlobalRngInClass();
std::unique_ptr<Rule> createRandomizeMissingSuper();
std::unique_ptr<Rule> createConstraintImplicationMultiBit();

// OOP rules (additional)
std::unique_ptr<Rule> createAbstractClassInstantiation();

// Coverage rules (additional)
std::unique_ptr<Rule> createIgnoreBinsOverlapsIllegalBins();

// Types rules (additional)
std::unique_ptr<Rule> createRedundantCastToSameType();
std::unique_ptr<Rule> createArrayLiteralSizeMismatch();

// UVM reporting rules (additional)
std::unique_ptr<Rule> createLowVerbosityDebugReport();

// Cluster 12 rules
std::unique_ptr<Rule> createForkLoopVariableCapture();
std::unique_ptr<Rule> createDisableIffComplexExpression();
std::unique_ptr<Rule> createMixedClockEdgesInProperty();
std::unique_ptr<Rule> createRandcInDynamicContainer();
std::unique_ptr<Rule> createParameterizedClassFactoryRegistration();
std::unique_ptr<Rule> createUncheckedPSequencerCast();
std::unique_ptr<Rule> createCoverpointWeightZero();
std::unique_ptr<Rule> createDynamicArrayNewInLoop();

// Cluster 13 rules
std::unique_ptr<Rule> createSuperNewNotFirstStatement();
std::unique_ptr<Rule> createNonConsecutiveRepetitionZeroBound();
std::unique_ptr<Rule> createConstraintInsideInvertedRange();
std::unique_ptr<Rule> createMissingIffOnCovergroupClock();
std::unique_ptr<Rule> createDirectMirrorSetWithoutWrite();
std::unique_ptr<Rule> createBlockingEventTriggerInClockedBlock();
std::unique_ptr<Rule> createUnboundedDynamicArrayInConstraint();
std::unique_ptr<Rule> createSequenceStartOnNullSequencer();

// Cluster 14 rules
std::unique_ptr<Rule> createMultiClockConcurrentAssertionDomain();
std::unique_ptr<Rule> createConstraintCallingNonPureFunction();
std::unique_ptr<Rule> createOverlappingRangesInCoverpointBin();
std::unique_ptr<Rule> createTaskWithNonVoidReturnType();
std::unique_ptr<Rule> createProceduralAssignmentAsSubExpression();
std::unique_ptr<Rule> createComponentNullParentInCreate();
std::unique_ptr<Rule> createMissingBackdoorAccessPathDefinition();
std::unique_ptr<Rule> createVirtualMethodSignatureMismatch();

// SVA & Assertion rules
std::unique_ptr<Rule> createImmediateAssertInCombinatorialLogic();

// Style rules
std::unique_ptr<Rule> createMacroTrailingSemicolon();
std::unique_ptr<Rule> createClassMemberPrefix();
std::unique_ptr<Rule> createTodoCommentTracker();
std::unique_ptr<Rule> createUvmCreateStringMismatch();
std::unique_ptr<Rule> createConstraintNameSuffix();
std::unique_ptr<Rule> createTypedefNameSuffix();
std::unique_ptr<Rule> createMandatoryMatchingEndLabel();
std::unique_ptr<Rule> createMismatchedEndLabel();
std::unique_ptr<Rule> createExternMethodThreshold();
std::unique_ptr<Rule> createExplicitLiteralWidth();
std::unique_ptr<Rule> createOneStatementPerLine();
std::unique_ptr<Rule> createSpaceKeywordParen();
std::unique_ptr<Rule> createIncludeGuard();
std::unique_ptr<Rule> createMethodBannerStyle();
std::unique_ptr<Rule> createSectionHeaderBanner();
std::unique_ptr<Rule> createConfigurableNamingEngine();
std::unique_ptr<Rule> createConfigurableStyleEngine();

// Deprecated & Modernization rules
std::unique_ptr<Rule> createProgramBlockDeprecation();
std::unique_ptr<Rule> createDefparamDeprecation();
std::unique_ptr<Rule> createLegacyAlwaysBlock();
std::unique_ptr<Rule> createRandomSystemFunction();
std::unique_ptr<Rule> createProceduralContinuousAssign();
std::unique_ptr<Rule> createNonAnsiPortList();
std::unique_ptr<Rule> createUvmDeprecatedConfigCalls();
std::unique_ptr<Rule> createUvmDeprecatedReportServer();
std::unique_ptr<Rule> createUvmDeprecatedPrinterKnobs();
std::unique_ptr<Rule> createUvmDeprecatedTestDone();
std::unique_ptr<Rule> createUvmBannedFieldMacros();
std::unique_ptr<Rule> createUvmBannedUvmDoMacros();
std::unique_ptr<Rule> createLegacyRegWireInTestbench();

// Testbench Bug-Hunting & Correctness rules
std::unique_ptr<Rule> createTautologicalCheckCompare();
std::unique_ptr<Rule> createOffByOneQueueIndex();
std::unique_ptr<Rule> createScoreboardSilentDrop();
std::unique_ptr<Rule> createScoreboardLeftoverQueue();
std::unique_ptr<Rule> createInlineShadowNoLocal();
std::unique_ptr<Rule> createNonRandFieldConstrained();
std::unique_ptr<Rule> createRandcMemoryExplosion();
std::unique_ptr<Rule> createTryPutIgnoredReturn();
std::unique_ptr<Rule> createDrivingDuringReset();
std::unique_ptr<Rule> createSequenceInFunctionPhase();

// Frontier Verification Rules (Portability, SVA, Security, RAL, DPI)
std::unique_ptr<Rule> createHardcodedHostPaths();
std::unique_ptr<Rule> createProprietaryVendorCalls();
std::unique_ptr<Rule> createSvaUnboundedDelay();
std::unique_ptr<Rule> createSvaVacuousAntecedent();
std::unique_ptr<Rule> createPlaintextKeyInLogs();
std::unique_ptr<Rule> createRalOrphanRegister();
std::unique_ptr<Rule> createRalAddressCollision();
std::unique_ptr<Rule> createDpiTypeWidthAlignment();

// Architecture & Methodology Blindspot Rules
std::unique_ptr<Rule> createGlobalUnitPollution();
std::unique_ptr<Rule> createHeaderMacroLeak();
std::unique_ptr<Rule> createNonExhaustiveEnumCase();
std::unique_ptr<Rule> createSequenceAbortSafety();
std::unique_ptr<Rule> createUvmDanglingConfigString();

// Simulation Performance & Queue rules
std::unique_ptr<Rule> createQueueConcatAppend();
std::unique_ptr<Rule> createQueueSlicePopFirst();
std::unique_ptr<Rule> createUnshieldedVerbosityFormatEval();
std::unique_ptr<Rule> createNbaOnClassProperty();

// Code Smells & Maintainability rules
std::unique_ptr<Rule> createScratchpadClassProperty();
std::unique_ptr<Rule> createMagicDelayInSequence();
std::unique_ptr<Rule> createGenericReportId();
std::unique_ptr<Rule> createCyclomaticComplexityGodMethod();

// Advanced Bug-Hunting & Architectural Rules
std::unique_ptr<Rule> createEqualityWithXz();
std::unique_ptr<Rule> createDisableIffReset();
std::unique_ptr<Rule> createBitSelectOutOfBounds();
std::unique_ptr<Rule> createNonInvertibleConstraintOp();
std::unique_ptr<Rule> createUnreferencedCovergroup();
std::unique_ptr<Rule> createUnlabeledAssertion();
std::unique_ptr<Rule> createConcurrentAssertInFunction();
std::unique_ptr<Rule> createAssertWithoutElse();
std::unique_ptr<Rule> createPartSelectReversed();
std::unique_ptr<Rule> createUnsizedLiteralInConcat();
std::unique_ptr<Rule> createCrossSameCoverpoint();
std::unique_ptr<Rule> createStaticConstraint();
std::unique_ptr<Rule> createCyclicSolveBefore();
std::unique_ptr<Rule> createRedundantConstraint();
std::unique_ptr<Rule> createProhibitedDisplayInUvm();
std::unique_ptr<Rule> createSuperBuildPhaseOrder();
std::unique_ptr<Rule> createComponentNameMismatch();
std::unique_ptr<Rule> createEmptySequenceBody();
std::unique_ptr<Rule> createUncheckedFopen();
std::unique_ptr<Rule> createNonBlockingInFunction();
std::unique_ptr<Rule> createZeroWeightCoverpoint();
std::unique_ptr<Rule> createTimeInAssertionAction();
std::unique_ptr<Rule> createUnboundedTransitionBins();
std::unique_ptr<Rule> createUvmConfigDbGlobalWildcard();
std::unique_ptr<Rule> createStringKeyAssociativeArray();
std::unique_ptr<Rule> createUvmComponentNamingConvention();
std::unique_ptr<Rule> createTrailingWhitespace();
std::unique_ptr<Rule> createZeroWeightDistribution();
std::unique_ptr<Rule> createIllegalBinOverlap();
std::unique_ptr<Rule> createUncheckedGetResponse();
std::unique_ptr<Rule> createMaxLineLength();
std::unique_ptr<Rule> createPackageNamingConvention();
std::unique_ptr<Rule> createInterfaceNamingConvention();
std::unique_ptr<Rule> createBlockingAssignmentToInterface();
std::unique_ptr<Rule> createAssociativeArrayMutatingRead();
std::unique_ptr<Rule> createClockingBlockOutputReadSkew();
std::unique_ptr<Rule> createArrayLocatorEmptyQueueDeref();
std::unique_ptr<Rule> createUnusedPureRoutineResult();
std::unique_ptr<Rule> createForkInsideFunction();
std::unique_ptr<Rule> createIntraAssignmentDelay();
std::unique_ptr<Rule> createZeroDelayEventTrigger();
std::unique_ptr<Rule> createStreamingConcatenationSizeMismatch();
std::unique_ptr<Rule> createCastTypeInspectionLadder();
std::unique_ptr<Rule> createMutableObjectGetterLeak();
std::unique_ptr<Rule> createUvmTransactionAliasing();
std::unique_ptr<Rule> createCovergroupCrossBinExplosion();
std::unique_ptr<Rule> createLspConstraintContradiction();
std::unique_ptr<Rule> createUnreadClassProperty();
std::unique_ptr<Rule> createUnwrittenClassProperty();
std::unique_ptr<Rule> createUnrestoredRandMode();
std::unique_ptr<Rule> createPhaseObjectionMismatch();
std::unique_ptr<Rule> createRepeatLoopWithoutTiming();
std::unique_ptr<Rule> createConfigDbGetUnchecked();
std::unique_ptr<Rule> createEmptyCovergroup();
std::unique_ptr<Rule> createEmptyRandomizeWith();
std::unique_ptr<Rule> createConstantWaitCondition();
std::unique_ptr<Rule> createTimescalePrecisionMismatch();
std::unique_ptr<Rule> createNonShortCircuitLogical();
std::unique_ptr<Rule> createUvmPhaseJumpHazard();
std::unique_ptr<Rule> createForeachLoopVariableShadowing();
std::unique_ptr<Rule> createSemaphoreZeroKey();
std::unique_ptr<Rule> createSequencePriorityRange();
std::unique_ptr<Rule> createUnconstrainedRandVariable();
std::unique_ptr<Rule> createCaseWithoutDefault();
std::unique_ptr<Rule> createCrossWithEmptyBins();
std::unique_ptr<Rule> createNonVirtualInterfaceInClass();
std::unique_ptr<Rule> createUnusedGenerateBlock();
std::unique_ptr<Rule> createUvmTlmTryGetUnchecked();
std::unique_ptr<Rule> createEmptyAlwaysOrInitial();
std::unique_ptr<Rule> createTimeInNonTimePhase();
std::unique_ptr<Rule> createConcurrentRandomizeInFork();
std::unique_ptr<Rule> createUvmClassTypeSuffix();
std::unique_ptr<Rule> createEventTriggerInAlwaysComb();
std::unique_ptr<Rule> createCoverpointDefaultBinHazard();
std::unique_ptr<Rule> createEmptyVirtualMethodWithoutPure();
std::unique_ptr<Rule> createAssignmentInExpression();
std::unique_ptr<Rule> createSequenceItemDefaultNew();
std::unique_ptr<Rule> createUnboundedMailboxInstantiation();
std::unique_ptr<Rule> createCrossWithAllBinsIgnored();
std::unique_ptr<Rule> createEmptyForkJoin();
std::unique_ptr<Rule> createSelfComparison();
std::unique_ptr<Rule> createMissingTimescaleInModule();
std::unique_ptr<Rule> createStringConcatInLoop();
std::unique_ptr<Rule> createClassInstantiationInLoop();
std::unique_ptr<Rule> createRandcStateExhaustion();
std::unique_ptr<Rule> createSequenceBodyWithoutStartItem();
std::unique_ptr<Rule> createMutexTryGetWithoutCheck();
std::unique_ptr<Rule> createAtLeastZeroOrNegative();
std::unique_ptr<Rule> createDowncastWithoutDynamicCast();
std::unique_ptr<Rule> createCasexProhibited();
std::unique_ptr<Rule> createEmptyTaskOrFunction();
std::unique_ptr<Rule> createUvmTlmPortSuffix();
std::unique_ptr<Rule> createWaitForkWithoutFork();
std::unique_ptr<Rule> createRandomizeInConstructor();
std::unique_ptr<Rule> createSequenceItemCreateWithoutType();
std::unique_ptr<Rule> createChainedAssignmentAliasing();
std::unique_ptr<Rule> createWideCoverpointWithoutBins();
std::unique_ptr<Rule> createUnreferencedParameter();
std::unique_ptr<Rule> createModifyingLoopIndexInsideFor();
std::unique_ptr<Rule> createObjectionInNonRunPhase();
std::unique_ptr<Rule> createObjectionControlFlowMismatch();
std::unique_ptr<Rule> createConfigDbCrossComponentResolver();
std::unique_ptr<Rule> createTlmTopologyChecker();
std::unique_ptr<Rule> createVirtualSequencerWiringChecker();
std::unique_ptr<Rule> createAnalysisTransactionMutation();
std::unique_ptr<Rule> createBuildPhaseCrossComponentAccess();
std::unique_ptr<Rule> createRalPredictorAdapterWiring();
std::unique_ptr<Rule> createSequenceItemTypeCompatibility();
std::unique_ptr<Rule> createRalMapAddressIntervalOverlap();
std::unique_ptr<Rule> createAnalysisFifoDrainStarvation();
std::unique_ptr<Rule> createCircularThreadWaitDeadlock();
std::unique_ptr<Rule> createVirtualInterfaceModportDriveInversion();
std::unique_ptr<Rule> createOrphanSubsequenceObjectionHang();

// Software engineering rules (bug patterns, error handling, design, metrics)
std::unique_ptr<Rule> createIdenticalBranches();
std::unique_ptr<Rule> createMismatchedFieldInCopyCompare();
std::unique_ptr<Rule> createRepeatedElseIfCondition();
std::unique_ptr<Rule> createSelfAssignment();
std::unique_ptr<Rule> createMutablePackageState();
std::unique_ptr<Rule> createTypeSwitchByName();
std::unique_ptr<Rule> createBlanketErrorDemotion();
std::unique_ptr<Rule> createCheckerCannotFail();
std::unique_ptr<Rule> createFailureNotReported();
std::unique_ptr<Rule> createFixedDelayBeforeDrop();
std::unique_ptr<Rule> createProcessTerminationInClass();
std::unique_ptr<Rule> createCognitiveComplexity();
std::unique_ptr<Rule> createComplexCondition();
std::unique_ptr<Rule> createDeepNesting();
std::unique_ptr<Rule> createLongRoutine();
std::unique_ptr<Rule> createTooManyArguments();
std::unique_ptr<Rule> createLockNotReleased();
std::unique_ptr<Rule> createFileIncludedInMultiplePackages();
std::unique_ptr<Rule> createGodClass();
// Methodology guide rules (docs/methodology/)
std::unique_ptr<Rule> createAgentIgnoresIsActive();
std::unique_ptr<Rule> createAgentWithoutConfigObject();
std::unique_ptr<Rule> createHardcodedTopology();
std::unique_ptr<Rule> createAbsoluteConfigDbPath();
std::unique_ptr<Rule> createOverrideByNameString();
std::unique_ptr<Rule> createIncompleteItemContract();
std::unique_ptr<Rule> createDriverGeneratesStimulus();
std::unique_ptr<Rule> createCoverageOutsideObservers();
std::unique_ptr<Rule> createDutCheckingInAgent();
std::unique_ptr<Rule> createTestBuildsLeafComponents();
std::unique_ptr<Rule> createMonitorDependsOnActivePath();
std::unique_ptr<Rule> createHandleToEnvOrTest();
std::unique_ptr<Rule> createForeignComponentStateWrite();
std::unique_ptr<Rule> createHierarchicalReferenceInClass();
std::unique_ptr<Rule> createCallbackNotRegistered();
std::unique_ptr<Rule> createIncompleteRegAdapter();
std::unique_ptr<Rule> createCheckFailureAsWarning();
std::unique_ptr<Rule> createFatalInCheckerPath();
std::unique_ptr<Rule> createDefineForConstant();
std::unique_ptr<Rule> createMacroAsRoutine();
std::unique_ptr<Rule> createIfdefInReusableClass();
std::unique_ptr<Rule> createStringAsEnum();
std::unique_ptr<Rule> createManualBitFieldSlicing();
std::unique_ptr<Rule> createImplicitEnumBaseType();
std::unique_ptr<Rule> createLegacyIntegerType();
std::unique_ptr<Rule> createFunctionOutputArguments();
std::unique_ptr<Rule> createStringModeParameter();
std::unique_ptr<Rule> createFileClassNameMismatch();
std::unique_ptr<Rule> createWildcardImportOutsidePackage();
std::unique_ptr<Rule> createUnusedPackageImport();
std::unique_ptr<Rule> createPackageLayerInversion();
std::unique_ptr<Rule> createUnstablePackageDependedOn();
std::unique_ptr<Rule> createConfigDbAsRuntimeChannel();
std::unique_ptr<Rule> createScatteredConfigKeyLiteral();
std::unique_ptr<Rule> createSharedConfigMutatedAtRuntime();
std::unique_ptr<Rule> createHierarchyReachThrough();
std::unique_ptr<Rule> createLayerDependency();
std::unique_ptr<Rule> createUseAfterFailedNullCheck();
std::unique_ptr<Rule> createIntegerDivisionToReal();
std::unique_ptr<Rule> createRealEquality();
std::unique_ptr<Rule> createSuspiciousArgumentOrder();
std::unique_ptr<Rule> createPlusargsOutsideTest();
std::unique_ptr<Rule> createStringHierarchyLookup();
std::unique_ptr<Rule> createLowCohesion();
std::unique_ptr<Rule> createHighCoupling();
std::unique_ptr<Rule> createDeepInheritance();
std::unique_ptr<Rule> createOverrideOnlyCallsSuper();
std::unique_ptr<Rule> createEmptyOverrideOfConcrete();
std::unique_ptr<Rule> createLargeClass();
std::unique_ptr<Rule> createFileHandleNotClosed();
std::unique_ptr<Rule> createUnusedRoutineArgument();
std::unique_ptr<Rule> createMagicNumber();
std::unique_ptr<Rule> createPositionalFlagLiterals();
std::unique_ptr<Rule> createRefArgumentNeverWritten();
std::unique_ptr<Rule> createPollingLoop();
std::unique_ptr<Rule> createSharedWriteFromForks();
std::unique_ptr<Rule> createUnboundedThreadSpawn();
std::unique_ptr<Rule> createQueryChangesState();
std::unique_ptr<Rule> createRepeatedEnumSwitch();
std::unique_ptr<Rule> createConstantReseed();
std::unique_ptr<Rule> createCommentedOutCode();
std::unique_ptr<Rule> createRedundantBoolean();
std::unique_ptr<Rule> createUndocumentedExtensionPoint();

void registerBuiltinRules() {
    static bool registered = false;
    if (registered) return;
    registered = true;

    auto& reg = RuleRegistry::instance();

    // UVM
    reg.registerRule(createUvmComponentDirectNew());
    reg.registerRule(createUvmMissingSuperPhase());
    reg.registerRule(createUvmTimeInFunctionPhase());
    reg.registerRule(createUvmConfigDbNullContext());
    reg.registerRule(createUvmFactoryMacroMismatch());
    reg.registerRule(createUvmObjectionInLoop());
    reg.registerRule(createUvmFactoryMissingUtils());
    reg.registerRule(createUvmPhaseSignatureMismatch());
    reg.registerRule(createUvmSequenceDirectVifAccess());
    reg.registerRule(createUvmUnpairedObjection());
    reg.registerRule(createUvmConfigDbTypeMismatch());
    reg.registerRule(createUvmUnconnectedTlmPort());
    reg.registerRule(createUvmSequencePrePostBodyMisuse());
    reg.registerRule(createUvmPSequencerAbuse());
    reg.registerRule(createObserverStateMutationInWrite());
    reg.registerRule(createDirectRalMirrorTampering());
    reg.registerRule(createObjectionControlFlowMismatch());
    reg.registerRule(createConfigDbCrossComponentResolver());
    reg.registerRule(createTlmTopologyChecker());
    reg.registerRule(createVirtualSequencerWiringChecker());
    reg.registerRule(createAnalysisTransactionMutation());
    reg.registerRule(createBuildPhaseCrossComponentAccess());
    reg.registerRule(createRalPredictorAdapterWiring());
    reg.registerRule(createSequenceItemTypeCompatibility());
    reg.registerRule(createRalMapAddressIntervalOverlap());
    reg.registerRule(createAnalysisFifoDrainStarvation());
    reg.registerRule(createCircularThreadWaitDeadlock());
    reg.registerRule(createVirtualInterfaceModportDriveInversion());
    reg.registerRule(createOrphanSubsequenceObjectionHang());

    // OOP
    reg.registerRule(createNonVirtualMethodOverride());
    reg.registerRule(createUncheckedDynamicCast());
    reg.registerRule(createMissingSuperNew());
    reg.registerRule(createVariableShadowing());
    reg.registerRule(createDuplicateMethodBody());
    reg.registerRule(createVirtualCallInConstructor());
    reg.registerRule(createClassShallowCopyAliasing());
    reg.registerRule(createStaticMemberInUvmComponent());

    // Concurrency
    reg.registerRule(createForkLoopVariableShared());
    reg.registerRule(createStaticRoutineVariableLifetime());
    reg.registerRule(createZeroDelayEventWaitLoop());

    // Performance
    reg.registerRule(createPassArrayByValue());
    reg.registerRule(createArrayReductionWidthTruncation());
    reg.registerRule(createDynArrayIncrementalResizeInLoop());
    reg.registerRule(createProceduralZeroDelay());

    // DeadCode
    reg.registerRule(createUnusedVariable());
    reg.registerRule(createUnreachableStatementsPostReturn());
    reg.registerRule(createAccidentalEmptyConditional());
    reg.registerRule(createConsecutiveDeadStore());

    // Types
    reg.registerRule(createMixedSignComparison());

    // Randomization
    reg.registerRule(createUncheckedRandomize());
    reg.registerRule(createGlobalRngInClass());
    reg.registerRule(createRandomizeMissingSuper());

    // SVA
    reg.registerRule(createImmediateAssertInCombinatorialLogic());

    // Style
    reg.registerRule(createMacroTrailingSemicolon());
    reg.registerRule(createClassMemberPrefix());
    reg.registerRule(createTodoCommentTracker());
    reg.registerRule(createUvmCreateStringMismatch());
    reg.registerRule(createConstraintNameSuffix());
    reg.registerRule(createTypedefNameSuffix());
    reg.registerRule(createMandatoryMatchingEndLabel());
    reg.registerRule(createMismatchedEndLabel());
    reg.registerRule(createExternMethodThreshold());
    reg.registerRule(createExplicitLiteralWidth());
    reg.registerRule(createOneStatementPerLine());
    reg.registerRule(createSpaceKeywordParen());
    reg.registerRule(createIncludeGuard());
    reg.registerRule(createMethodBannerStyle());
    reg.registerRule(createSectionHeaderBanner());

    // Deprecated & Modernization
    reg.registerRule(createProgramBlockDeprecation());
    reg.registerRule(createDefparamDeprecation());
    reg.registerRule(createLegacyAlwaysBlock());
    reg.registerRule(createRandomSystemFunction());
    reg.registerRule(createProceduralContinuousAssign());
    reg.registerRule(createNonAnsiPortList());
    reg.registerRule(createUvmDeprecatedConfigCalls());
    reg.registerRule(createUvmDeprecatedReportServer());
    reg.registerRule(createUvmDeprecatedPrinterKnobs());
    reg.registerRule(createUvmDeprecatedTestDone());
    reg.registerRule(createUvmBannedFieldMacros());
    reg.registerRule(createUvmBannedUvmDoMacros());
    reg.registerRule(createLegacyRegWireInTestbench());

    // Testbench Bug-Hunting & Correctness
    reg.registerRule(createTautologicalCheckCompare());
    reg.registerRule(createOffByOneQueueIndex());
    reg.registerRule(createScoreboardSilentDrop());
    reg.registerRule(createScoreboardLeftoverQueue());
    reg.registerRule(createInlineShadowNoLocal());
    reg.registerRule(createNonRandFieldConstrained());
    reg.registerRule(createRandcMemoryExplosion());
    reg.registerRule(createTryPutIgnoredReturn());
    reg.registerRule(createDrivingDuringReset());
    reg.registerRule(createSequenceInFunctionPhase());

    // Frontier Verification Rules
    reg.registerRule(createHardcodedHostPaths());
    reg.registerRule(createProprietaryVendorCalls());
    reg.registerRule(createSvaUnboundedDelay());
    reg.registerRule(createSvaVacuousAntecedent());
    reg.registerRule(createPlaintextKeyInLogs());
    reg.registerRule(createRalOrphanRegister());
    reg.registerRule(createRalAddressCollision());
    reg.registerRule(createDpiTypeWidthAlignment());

    // Architecture & Methodology Blindspot Rules
    reg.registerRule(createGlobalUnitPollution());
    reg.registerRule(createHeaderMacroLeak());
    reg.registerRule(createNonExhaustiveEnumCase());
    reg.registerRule(createSequenceAbortSafety());
    reg.registerRule(createUvmDanglingConfigString());

    // Simulation Performance & Queues
    reg.registerRule(createQueueConcatAppend());
    reg.registerRule(createQueueSlicePopFirst());
    reg.registerRule(createUnshieldedVerbosityFormatEval());
    reg.registerRule(createNbaOnClassProperty());

    // Code Smells & Maintainability
    reg.registerRule(createScratchpadClassProperty());
    reg.registerRule(createMagicDelayInSequence());
    reg.registerRule(createGenericReportId());
    reg.registerRule(createCyclomaticComplexityGodMethod());

    // Advanced Bug-Hunting & Architectural Rules
    reg.registerRule(createEqualityWithXz());
    reg.registerRule(createDisableIffReset());
    reg.registerRule(createBitSelectOutOfBounds());
    reg.registerRule(createNonInvertibleConstraintOp());
    reg.registerRule(createUnreferencedCovergroup());
    reg.registerRule(createUnlabeledAssertion());
    reg.registerRule(createConcurrentAssertInFunction());
    reg.registerRule(createAssertWithoutElse());
    reg.registerRule(createPartSelectReversed());
    reg.registerRule(createUnsizedLiteralInConcat());
    reg.registerRule(createCrossSameCoverpoint());
    reg.registerRule(createStaticConstraint());
    reg.registerRule(createCyclicSolveBefore());
    reg.registerRule(createRedundantConstraint());
    reg.registerRule(createProhibitedDisplayInUvm());
    reg.registerRule(createSuperBuildPhaseOrder());
    reg.registerRule(createComponentNameMismatch());
    reg.registerRule(createEmptySequenceBody());
    reg.registerRule(createUncheckedFopen());
    reg.registerRule(createNonBlockingInFunction());
    reg.registerRule(createZeroWeightCoverpoint());
    reg.registerRule(createTimeInAssertionAction());
    reg.registerRule(createUnboundedTransitionBins());
    reg.registerRule(createUvmConfigDbGlobalWildcard());
    reg.registerRule(createStringKeyAssociativeArray());
    reg.registerRule(createUvmComponentNamingConvention());
    reg.registerRule(createTrailingWhitespace());
    reg.registerRule(createZeroWeightDistribution());
    reg.registerRule(createIllegalBinOverlap());
    reg.registerRule(createUncheckedGetResponse());
    reg.registerRule(createMaxLineLength());
    reg.registerRule(createPackageNamingConvention());
    reg.registerRule(createInterfaceNamingConvention());
    reg.registerRule(createBlockingAssignmentToInterface());
    reg.registerRule(createAssociativeArrayMutatingRead());
    reg.registerRule(createClockingBlockOutputReadSkew());
    reg.registerRule(createArrayLocatorEmptyQueueDeref());
    reg.registerRule(createUnusedPureRoutineResult());
    reg.registerRule(createForkInsideFunction());
    reg.registerRule(createIntraAssignmentDelay());
    reg.registerRule(createZeroDelayEventTrigger());
    reg.registerRule(createStreamingConcatenationSizeMismatch());
    reg.registerRule(createCastTypeInspectionLadder());
    reg.registerRule(createMutableObjectGetterLeak());
    reg.registerRule(createUvmTransactionAliasing());
    reg.registerRule(createCovergroupCrossBinExplosion());
    reg.registerRule(createLspConstraintContradiction());
    reg.registerRule(createUnreadClassProperty());
    reg.registerRule(createUnwrittenClassProperty());
    reg.registerRule(createUnrestoredRandMode());
    reg.registerRule(createPhaseObjectionMismatch());
    reg.registerRule(createRepeatLoopWithoutTiming());
    reg.registerRule(createConfigDbGetUnchecked());
    reg.registerRule(createEmptyCovergroup());
    reg.registerRule(createEmptyRandomizeWith());
    reg.registerRule(createConstantWaitCondition());
    reg.registerRule(createTimescalePrecisionMismatch());
    reg.registerRule(createNonShortCircuitLogical());
    reg.registerRule(createUvmPhaseJumpHazard());
    reg.registerRule(createForeachLoopVariableShadowing());
    reg.registerRule(createSemaphoreZeroKey());
    reg.registerRule(createSequencePriorityRange());
    reg.registerRule(createUnconstrainedRandVariable());
    reg.registerRule(createCaseWithoutDefault());
    reg.registerRule(createCrossWithEmptyBins());
    reg.registerRule(createNonVirtualInterfaceInClass());
    reg.registerRule(createUnusedGenerateBlock());
    reg.registerRule(createUvmTlmTryGetUnchecked());
    reg.registerRule(createEmptyAlwaysOrInitial());
    reg.registerRule(createTimeInNonTimePhase());
    reg.registerRule(createConcurrentRandomizeInFork());
    reg.registerRule(createUvmClassTypeSuffix());
    reg.registerRule(createEventTriggerInAlwaysComb());
    reg.registerRule(createCoverpointDefaultBinHazard());
    reg.registerRule(createEmptyVirtualMethodWithoutPure());
    reg.registerRule(createAssignmentInExpression());
    reg.registerRule(createSequenceItemDefaultNew());
    reg.registerRule(createUnboundedMailboxInstantiation());
    reg.registerRule(createCrossWithAllBinsIgnored());
    reg.registerRule(createEmptyForkJoin());
    reg.registerRule(createSelfComparison());
    reg.registerRule(createMissingTimescaleInModule());
    reg.registerRule(createStringConcatInLoop());
    reg.registerRule(createClassInstantiationInLoop());
    reg.registerRule(createRandcStateExhaustion());
    reg.registerRule(createSequenceBodyWithoutStartItem());
    reg.registerRule(createMutexTryGetWithoutCheck());
    reg.registerRule(createAtLeastZeroOrNegative());
    reg.registerRule(createDowncastWithoutDynamicCast());
    reg.registerRule(createCasexProhibited());
    reg.registerRule(createEmptyTaskOrFunction());
    reg.registerRule(createUvmTlmPortSuffix());
    reg.registerRule(createWaitForkWithoutFork());
    reg.registerRule(createRandomizeInConstructor());
    reg.registerRule(createSequenceItemCreateWithoutType());
    reg.registerRule(createChainedAssignmentAliasing());
    reg.registerRule(createWideCoverpointWithoutBins());
    reg.registerRule(createUnreferencedParameter());
    reg.registerRule(createModifyingLoopIndexInsideFor());
    reg.registerRule(createObjectionInNonRunPhase());
    reg.registerRule(createConfigurableNamingEngine());
    reg.registerRule(createConfigurableStyleEngine());
    reg.registerRule(createDisableForkWithoutScope());
    reg.registerRule(createConstraintImplicationMultiBit());
    reg.registerRule(createWaitModifiedInLoop());
    reg.registerRule(createAbstractClassInstantiation());
    reg.registerRule(createIgnoreBinsOverlapsIllegalBins());
    reg.registerRule(createRedundantCastToSameType());
    reg.registerRule(createArrayLiteralSizeMismatch());
    reg.registerRule(createLowVerbosityDebugReport());
    reg.registerRule(createForkLoopVariableCapture());
    reg.registerRule(createDisableIffComplexExpression());
    reg.registerRule(createMixedClockEdgesInProperty());
    reg.registerRule(createRandcInDynamicContainer());
    reg.registerRule(createParameterizedClassFactoryRegistration());
    reg.registerRule(createUncheckedPSequencerCast());
    reg.registerRule(createCoverpointWeightZero());
    reg.registerRule(createDynamicArrayNewInLoop());
    reg.registerRule(createSuperNewNotFirstStatement());
    reg.registerRule(createNonConsecutiveRepetitionZeroBound());
    reg.registerRule(createConstraintInsideInvertedRange());
    reg.registerRule(createMissingIffOnCovergroupClock());
    reg.registerRule(createDirectMirrorSetWithoutWrite());
    reg.registerRule(createBlockingEventTriggerInClockedBlock());
    reg.registerRule(createUnboundedDynamicArrayInConstraint());
    reg.registerRule(createSequenceStartOnNullSequencer());
    reg.registerRule(createMultiClockConcurrentAssertionDomain());
    reg.registerRule(createConstraintCallingNonPureFunction());
    reg.registerRule(createOverlappingRangesInCoverpointBin());
    reg.registerRule(createTaskWithNonVoidReturnType());
    reg.registerRule(createProceduralAssignmentAsSubExpression());
    reg.registerRule(createComponentNullParentInCreate());
    reg.registerRule(createMissingBackdoorAccessPathDefinition());
    reg.registerRule(createVirtualMethodSignatureMismatch());

    // Software engineering rules
    reg.registerRule(createQueryChangesState());
    reg.registerRule(createRepeatedEnumSwitch());
    reg.registerRule(createConstantReseed());
    reg.registerRule(createCommentedOutCode());
    reg.registerRule(createRedundantBoolean());
    reg.registerRule(createUndocumentedExtensionPoint());
    reg.registerRule(createPollingLoop());
    reg.registerRule(createSharedWriteFromForks());
    reg.registerRule(createUnboundedThreadSpawn());
    reg.registerRule(createFileHandleNotClosed());
    reg.registerRule(createUnusedRoutineArgument());
    reg.registerRule(createMagicNumber());
    reg.registerRule(createPositionalFlagLiterals());
    reg.registerRule(createRefArgumentNeverWritten());
    reg.registerRule(createLowCohesion());
    reg.registerRule(createHighCoupling());
    reg.registerRule(createDeepInheritance());
    reg.registerRule(createOverrideOnlyCallsSuper());
    reg.registerRule(createEmptyOverrideOfConcrete());
    reg.registerRule(createLargeClass());
    reg.registerRule(createIntegerDivisionToReal());
    reg.registerRule(createRealEquality());
    reg.registerRule(createSuspiciousArgumentOrder());
    reg.registerRule(createPlusargsOutsideTest());
    reg.registerRule(createStringHierarchyLookup());
    reg.registerRule(createLockNotReleased());
    reg.registerRule(createFileIncludedInMultiplePackages());
    reg.registerRule(createGodClass());
    reg.registerRule(createHierarchyReachThrough());
    reg.registerRule(createLayerDependency());
    reg.registerRule(createUseAfterFailedNullCheck());
    reg.registerRule(createIdenticalBranches());
    reg.registerRule(createMismatchedFieldInCopyCompare());
    reg.registerRule(createRepeatedElseIfCondition());
    reg.registerRule(createSelfAssignment());
    reg.registerRule(createMutablePackageState());
    reg.registerRule(createTypeSwitchByName());
    reg.registerRule(createBlanketErrorDemotion());
    reg.registerRule(createCheckerCannotFail());
    reg.registerRule(createFailureNotReported());
    reg.registerRule(createFixedDelayBeforeDrop());
    reg.registerRule(createProcessTerminationInClass());
    reg.registerRule(createCognitiveComplexity());
    reg.registerRule(createComplexCondition());
    reg.registerRule(createDeepNesting());
    reg.registerRule(createLongRoutine());
    reg.registerRule(createTooManyArguments());
    reg.registerRule(createAgentIgnoresIsActive());
    reg.registerRule(createAgentWithoutConfigObject());
    reg.registerRule(createHardcodedTopology());
    reg.registerRule(createAbsoluteConfigDbPath());
    reg.registerRule(createOverrideByNameString());
    reg.registerRule(createIncompleteItemContract());
    reg.registerRule(createDriverGeneratesStimulus());
    reg.registerRule(createCoverageOutsideObservers());
    reg.registerRule(createDutCheckingInAgent());
    reg.registerRule(createTestBuildsLeafComponents());
    reg.registerRule(createMonitorDependsOnActivePath());
    reg.registerRule(createHandleToEnvOrTest());
    reg.registerRule(createForeignComponentStateWrite());
    reg.registerRule(createHierarchicalReferenceInClass());
    reg.registerRule(createCallbackNotRegistered());
    reg.registerRule(createIncompleteRegAdapter());
    reg.registerRule(createCheckFailureAsWarning());
    reg.registerRule(createFatalInCheckerPath());
    reg.registerRule(createDefineForConstant());
    reg.registerRule(createMacroAsRoutine());
    reg.registerRule(createIfdefInReusableClass());
    reg.registerRule(createStringAsEnum());
    reg.registerRule(createManualBitFieldSlicing());
    reg.registerRule(createImplicitEnumBaseType());
    reg.registerRule(createLegacyIntegerType());
    reg.registerRule(createFunctionOutputArguments());
    reg.registerRule(createStringModeParameter());
    reg.registerRule(createFileClassNameMismatch());
    reg.registerRule(createWildcardImportOutsidePackage());
    reg.registerRule(createUnusedPackageImport());
    reg.registerRule(createPackageLayerInversion());
    reg.registerRule(createUnstablePackageDependedOn());
    reg.registerRule(createConfigDbAsRuntimeChannel());
    reg.registerRule(createScatteredConfigKeyLiteral());
    reg.registerRule(createSharedConfigMutatedAtRuntime());
}

} // namespace uvmcheck::rules
