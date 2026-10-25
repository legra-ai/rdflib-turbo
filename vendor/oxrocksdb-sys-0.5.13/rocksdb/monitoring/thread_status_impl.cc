// Copyright (c) 2011-present, Facebook, Inc.  All rights reserved.
//  This source code is licensed under both the GPLv2 (found in the
//  COPYING file in the root directory) and Apache 2.0 License
//  (found in the LICENSE.Apache file in the root directory).
//

#include <sstream>

#include "rocksdb/env.h"
#include "rocksdb/thread_status.h"
#include "util/string_util.h"
#include "util/thread_operation.h"

namespace ROCKSDB_NAMESPACE {

#ifndef NROCKSDB_THREAD_STATUS

// One definition: including thread-operation metadata must not construct per-TU copies.
OperationInfo global_operation_table[] = {
    {ThreadStatus::OP_UNKNOWN, ""},
    {ThreadStatus::OP_COMPACTION, "Compaction"},
    {ThreadStatus::OP_FLUSH, "Flush"},
    {ThreadStatus::OP_DBOPEN, "DBOpen"},
    {ThreadStatus::OP_GET, "Get"},
    {ThreadStatus::OP_MULTIGET, "MultiGet"},
    {ThreadStatus::OP_DBITERATOR, "DBIterator"},
    {ThreadStatus::OP_VERIFY_DB_CHECKSUM, "VerifyDBChecksum"},
    {ThreadStatus::OP_VERIFY_FILE_CHECKSUMS, "VerifyFileChecksums"},
    {ThreadStatus::OP_GETENTITY, "GetEntity"},
    {ThreadStatus::OP_MULTIGETENTITY, "MultiGetEntity"},
    {ThreadStatus::OP_GET_FILE_CHECKSUMS_FROM_CURRENT_MANIFEST,
     "GetFileChecksumsFromCurrentManifest"},

};

OperationStageInfo global_op_stage_table[] = {
    {ThreadStatus::STAGE_UNKNOWN, ""},
    {ThreadStatus::STAGE_FLUSH_RUN, "FlushJob::Run"},
    {ThreadStatus::STAGE_FLUSH_WRITE_L0, "FlushJob::WriteLevel0Table"},
    {ThreadStatus::STAGE_COMPACTION_PREPARE, "CompactionJob::Prepare"},
    {ThreadStatus::STAGE_COMPACTION_RUN, "CompactionJob::Run"},
    {ThreadStatus::STAGE_COMPACTION_PROCESS_KV,
     "CompactionJob::ProcessKeyValueCompaction"},
    {ThreadStatus::STAGE_COMPACTION_INSTALL, "CompactionJob::Install"},
    {ThreadStatus::STAGE_COMPACTION_SYNC_FILE,
     "CompactionJob::FinishCompactionOutputFile"},
    {ThreadStatus::STAGE_PICK_MEMTABLES_TO_FLUSH,
     "MemTableList::PickMemtablesToFlush"},
    {ThreadStatus::STAGE_MEMTABLE_ROLLBACK,
     "MemTableList::RollbackMemtableFlush"},
    {ThreadStatus::STAGE_MEMTABLE_INSTALL_FLUSH_RESULTS,
     "MemTableList::TryInstallMemtableFlushResults"},
};

StateInfo global_state_table[] = {
    {ThreadStatus::STATE_UNKNOWN, ""},
    {ThreadStatus::STATE_MUTEX_WAIT, "Mutex Wait"},
};

OperationProperty compaction_operation_properties[] = {
    {ThreadStatus::COMPACTION_JOB_ID, "JobID"},
    {ThreadStatus::COMPACTION_INPUT_OUTPUT_LEVEL, "InputOutputLevel"},
    {ThreadStatus::COMPACTION_PROP_FLAGS, "Manual/Deletion/Trivial"},
    {ThreadStatus::COMPACTION_TOTAL_INPUT_BYTES, "TotalInputBytes"},
    {ThreadStatus::COMPACTION_BYTES_READ, "BytesRead"},
    {ThreadStatus::COMPACTION_BYTES_WRITTEN, "BytesWritten"},
};

OperationProperty flush_operation_properties[] = {
    {ThreadStatus::FLUSH_JOB_ID, "JobID"},
    {ThreadStatus::FLUSH_BYTES_MEMTABLES, "BytesMemtables"},
    {ThreadStatus::FLUSH_BYTES_WRITTEN, "BytesWritten"}};

const bool ThreadStatus::kEnabled = true;

std::string ThreadStatus::GetThreadTypeName(
    ThreadStatus::ThreadType thread_type) {
  switch (thread_type) {
    case ThreadStatus::ThreadType::HIGH_PRIORITY:
      return "High Pri";
    case ThreadStatus::ThreadType::LOW_PRIORITY:
      return "Low Pri";
    case ThreadStatus::ThreadType::USER:
      return "User";
    case ThreadStatus::ThreadType::BOTTOM_PRIORITY:
      return "Bottom Pri";
    case ThreadStatus::ThreadType::NUM_THREAD_TYPES:
      assert(false);
  }
  return "Unknown";
}

const std::string& ThreadStatus::GetOperationName(
    ThreadStatus::OperationType op_type) {
  if (op_type < 0 || op_type >= NUM_OP_TYPES) {
    return global_operation_table[OP_UNKNOWN].name;
  }
  return global_operation_table[op_type].name;
}

const std::string& ThreadStatus::GetOperationStageName(
    ThreadStatus::OperationStage stage) {
  if (stage < 0 || stage >= NUM_OP_STAGES) {
    return global_op_stage_table[STAGE_UNKNOWN].name;
  }
  return global_op_stage_table[stage].name;
}

const std::string& ThreadStatus::GetStateName(
    ThreadStatus::StateType state_type) {
  if (state_type < 0 || state_type >= NUM_STATE_TYPES) {
    return global_state_table[STATE_UNKNOWN].name;
  }
  return global_state_table[state_type].name;
}

const std::string ThreadStatus::MicrosToString(uint64_t micros) {
  if (micros == 0) {
    return "";
  }
  const int kBufferLen = 100;
  char buffer[kBufferLen];
  AppendHumanMicros(micros, buffer, kBufferLen, false);
  return std::string(buffer);
}

const std::string& ThreadStatus::GetOperationPropertyName(
    ThreadStatus::OperationType op_type, int i) {
  static const std::string empty_str;
  switch (op_type) {
    case ThreadStatus::OP_COMPACTION:
      if (i >= NUM_COMPACTION_PROPERTIES) {
        return empty_str;
      }
      return compaction_operation_properties[i].name;
    case ThreadStatus::OP_FLUSH:
      if (i >= NUM_FLUSH_PROPERTIES) {
        return empty_str;
      }
      return flush_operation_properties[i].name;
    default:
      return empty_str;
  }
}

std::map<std::string, uint64_t> ThreadStatus::InterpretOperationProperties(
    ThreadStatus::OperationType op_type, const uint64_t* op_properties) {
  int num_properties;
  switch (op_type) {
    case OP_COMPACTION:
      num_properties = NUM_COMPACTION_PROPERTIES;
      break;
    case OP_FLUSH:
      num_properties = NUM_FLUSH_PROPERTIES;
      break;
    default:
      num_properties = 0;
  }

  std::map<std::string, uint64_t> property_map;
  for (int i = 0; i < num_properties; ++i) {
    if (op_type == OP_COMPACTION && i == COMPACTION_INPUT_OUTPUT_LEVEL) {
      property_map.insert({"BaseInputLevel", op_properties[i] >> 32});
      property_map.insert(
          {"OutputLevel", op_properties[i] % (uint64_t(1) << 32U)});
    } else if (op_type == OP_COMPACTION && i == COMPACTION_PROP_FLAGS) {
      property_map.insert({"IsManual", ((op_properties[i] & 2) >> 1)});
      property_map.insert({"IsDeletion", ((op_properties[i] & 4) >> 2)});
      property_map.insert({"IsTrivialMove", ((op_properties[i] & 8) >> 3)});
    } else {
      property_map.insert(
          {GetOperationPropertyName(op_type, i), op_properties[i]});
    }
  }
  return property_map;
}

#else
const bool ThreadStatus::kEnabled = false;

std::string ThreadStatus::GetThreadTypeName(
    ThreadStatus::ThreadType /*thread_type*/) {
  static std::string dummy_str = "";
  return dummy_str;
}

const std::string& ThreadStatus::GetOperationName(
    ThreadStatus::OperationType /*op_type*/) {
  static std::string dummy_str = "";
  return dummy_str;
}

const std::string& ThreadStatus::GetOperationStageName(
    ThreadStatus::OperationStage /*stage*/) {
  static std::string dummy_str = "";
  return dummy_str;
}

const std::string& ThreadStatus::GetStateName(
    ThreadStatus::StateType /*state_type*/) {
  static std::string dummy_str = "";
  return dummy_str;
}

const std::string ThreadStatus::MicrosToString(uint64_t /*op_elapsed_time*/) {
  static std::string dummy_str = "";
  return dummy_str;
}

const std::string& ThreadStatus::GetOperationPropertyName(
    ThreadStatus::OperationType /*op_type*/, int /*i*/) {
  static std::string dummy_str = "";
  return dummy_str;
}

std::map<std::string, uint64_t> ThreadStatus::InterpretOperationProperties(
    ThreadStatus::OperationType /*op_type*/,
    const uint64_t* /*op_properties*/) {
  return std::map<std::string, uint64_t>();
}

#endif  // !NROCKSDB_THREAD_STATUS
}  // namespace ROCKSDB_NAMESPACE
