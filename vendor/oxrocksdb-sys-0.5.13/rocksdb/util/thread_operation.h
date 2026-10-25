// Copyright (c) 2011-present, Facebook, Inc.  All rights reserved.
//  This source code is licensed under both the GPLv2 (found in the
//  COPYING file in the root directory) and Apache 2.0 License
//  (found in the LICENSE.Apache file in the root directory).
//
// This file defines the structures for thread operation and state.
// Thread operations are used to describe high level action of a
// thread such as doing compaction or flush, while thread state
// are used to describe lower-level action such as reading /
// writing a file or waiting for a mutex.  Operations and states
// are designed to be independent.  Typically, a thread usually involves
// in one operation and one state at any specific point in time.

#pragma once

#include <string>

#include "rocksdb/thread_status.h"

namespace ROCKSDB_NAMESPACE {

#ifndef NROCKSDB_THREAD_STATUS

// The structure that describes a major thread operation.
struct OperationInfo {
  const ThreadStatus::OperationType type;
  const std::string name;
};

// The global operation table.
//
// When updating a status of a thread, the pointer of the OperationInfo
// of the current ThreadStatusData will be pointing to one of the
// rows in this global table.
//
// Note that it's not designed to be constant as in the future we
// might consider adding global count to the OperationInfo.
extern OperationInfo global_operation_table[];

struct OperationStageInfo {
  const ThreadStatus::OperationStage stage;
  const std::string name;
};

// A table maintains the mapping from stage type to stage string.
// Note that the string must be changed accordingly when the
// associated function name changed.
extern OperationStageInfo global_op_stage_table[];

// The structure that describes a state.
struct StateInfo {
  const ThreadStatus::StateType type;
  const std::string name;
};

// The global state table.
//
// When updating a status of a thread, the pointer of the StateInfo
// of the current ThreadStatusData will be pointing to one of the
// rows in this global table.
extern StateInfo global_state_table[];

struct OperationProperty {
  int code;
  std::string name;
};

extern OperationProperty compaction_operation_properties[];

extern OperationProperty flush_operation_properties[];

#else

struct OperationInfo {};

struct StateInfo {};

#endif  // !NROCKSDB_THREAD_STATUS
}  // namespace ROCKSDB_NAMESPACE
