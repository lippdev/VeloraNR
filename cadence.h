// Scoped, on-demand DxgKrnl Present tracing. Called on a Rust background
// worker.
#include <array>
#include <evntcons.h>
#include <evntprov.h>
#include <evntrace.h>
#include <set>
#include <string>
#include <vector>
#pragma comment(lib, "advapi32.lib")

namespace cadence {
static std::atomic<DWORD> sidecar{0};
static std::atomic<bool> closing{false};
static std::mutex captureLock;
static constexpr GUID dxgkrnl{0x802ec45a,
                              0x1e99,
                              0x4b83,
                              {0x99, 0x20, 0x87, 0xc9, 0x82, 0x77, 0xba, 0x9d}};
using Sample = void (*)(void *, DWORD, UINT64, UINT64);

struct Reader {
  Sample sample;
  void *context;
  std::set<DWORD> pids;
};
static void WINAPI event(EVENT_RECORD *event) {
  auto &h = event->EventHeader;
  auto reader = static_cast<Reader *>(event->UserContext);
  if (!reader->pids.contains(h.ProcessId))
    return;
  if (!IsEqualGUID(h.ProviderId, dxgkrnl) || h.EventDescriptor.Id != 184 ||
      h.EventDescriptor.Version != 1)
    return;
  size_t ptrSize = (h.Flags & EVENT_HEADER_FLAG_32_BIT_HEADER) ? 4 : 8;
  // DxgKrnl/Present v1: hContext, hWindow, VidPnSourceId, FlipInterval, Flags,
  // ReturnStatus.
  const auto data = static_cast<const BYTE *>(event->UserData);
  if (event->UserDataLength < 5 * sizeof(DWORD) + ptrSize)
    return;
  UINT64 chain = 0;
  DWORD result = 0;
  memcpy(&chain, data + sizeof(DWORD), ptrSize);
  memcpy(&result, data + 4 * sizeof(DWORD) + ptrSize, sizeof(result));
  if (!chain || result != 0)
    return;
  reader->sample(reader->context, h.ProcessId, chain, h.TimeStamp.QuadPart);
}

static ULONG enable(TRACEHANDLE session, DWORD engine) {
  std::array<DWORD, 2> pids{GetCurrentProcessId(), engine};
  // Filter to successful kernel submissions; group the samples by target HWND.
  struct Events {
    BOOLEAN include;
    UCHAR reserved;
    USHORT count;
    USHORT ids[1];
  } events{TRUE, 0, 1, {184}};
  EVENT_FILTER_DESCRIPTOR filters[2]{};
  auto &filter = filters[0];
  filters[1].Ptr = reinterpret_cast<ULONGLONG>(&events);
  filters[1].Size =
      FIELD_OFFSET(EVENT_FILTER_EVENT_ID, Events) + sizeof(USHORT);
  filters[1].Type = EVENT_FILTER_TYPE_EVENT_ID;
  filter.Ptr = reinterpret_cast<ULONGLONG>(pids.data());
  filter.Size = sizeof(DWORD) * (engine ? 2 : 1);
  filter.Type = EVENT_FILTER_TYPE_PID;
  ENABLE_TRACE_PARAMETERS parameters{};
  parameters.Version = ENABLE_TRACE_PARAMETERS_VERSION_2;
  parameters.EnableFilterDesc = filters;
  parameters.FilterDescCount = 2;
  return EnableTraceEx2(session, &dxgkrnl, EVENT_CONTROL_CODE_ENABLE_PROVIDER,
                        TRACE_LEVEL_VERBOSE, 0x08000000, 0, 0, &parameters);
}

static ULONG capture(const wchar_t *path, Sample sample, void *context,
                     ULONG *lost) {
  std::lock_guard lock(captureLock);
  if (closing.load())
    return ERROR_CANCELLED;
  std::wstring name =
      L"VeloraNR-DXGI-" + std::to_wstring(GetCurrentProcessId());
  std::wstring file(path);
  const auto size = sizeof(EVENT_TRACE_PROPERTIES) + 2 * 1024 * sizeof(wchar_t);
  std::vector<BYTE> buffer(size);
  auto p = reinterpret_cast<EVENT_TRACE_PROPERTIES *>(buffer.data());
  p->Wnode.BufferSize = static_cast<ULONG>(size);
  p->Wnode.Flags = WNODE_FLAG_TRACED_GUID;
  p->Wnode.ClientContext =
      1; // QPC clock; ProcessTrace converts timestamps to UTC.
  p->LogFileMode =
      EVENT_TRACE_FILE_MODE_SEQUENTIAL | EVENT_TRACE_NO_PER_PROCESSOR_BUFFERING;
  p->FlushTimer = 1;
  p->MaximumFileSize = 64;
  p->BufferSize = 64;
  p->MinimumBuffers = 16;
  p->MaximumBuffers = 128;
  p->LoggerNameOffset = sizeof(EVENT_TRACE_PROPERTIES);
  p->LogFileNameOffset = p->LoggerNameOffset + 1024 * sizeof(wchar_t);
  if (file.size() >= 1024)
    return ERROR_FILENAME_EXCED_RANGE;
  memcpy(buffer.data() + p->LogFileNameOffset, file.c_str(),
         (file.size() + 1) * sizeof(wchar_t));
  TRACEHANDLE session{};
  ULONG error = StartTraceW(&session, name.c_str(), p);
  if (error != ERROR_SUCCESS)
    return error;
  DWORD engine = sidecar.load();
  Reader reader{sample, context, {GetCurrentProcessId()}};
  if (engine)
    reader.pids.insert(engine);
  error = enable(session, engine);
  const auto end = GetTickCount64() + 15000;
  while (error == ERROR_SUCCESS && GetTickCount64() < end) {
    if (closing.load()) {
      error = ERROR_CANCELLED;
      break;
    }
    Sleep(100);
    auto next = sidecar.load();
    if (next != engine) {
      engine = next;
      if (engine)
        reader.pids.insert(engine);
      error = enable(session, engine);
    }
  }
  auto stop = ControlTraceW(session, nullptr, p, EVENT_TRACE_CONTROL_STOP);
  if (error != ERROR_SUCCESS)
    return error;
  log::info("session-ui",
            std::format("traceStop={} eventsLost={} buffersWritten={}", stop,
                        p->EventsLost, p->BuffersWritten));
  if (stop != ERROR_SUCCESS && stop != ERROR_MORE_DATA)
    return stop;
  *lost = p->EventsLost;

  EVENT_TRACE_LOGFILEW log{};
  log.LogFileName = file.data();
  log.ProcessTraceMode = PROCESS_TRACE_MODE_EVENT_RECORD;
  log.EventRecordCallback = event;
  log.Context = &reader;
  auto trace = OpenTraceW(&log);
  if (trace == INVALID_PROCESSTRACE_HANDLE)
    return GetLastError();
  error = ProcessTrace(&trace, 1, nullptr, nullptr);
  CloseTrace(trace);
  return error;
}
} // namespace cadence
API void portal_cancel_measure() {
  cadence::closing.store(true);
  std::lock_guard lock(cadence::captureLock);
}
API void portal_motion_pid(DWORD pid) { cadence::sidecar.store(pid); }
API ULONG portal_measure(const wchar_t *path, cadence::Sample sample,
                         void *context, ULONG *lost) noexcept {
  try {
    auto result = cadence::capture(path, sample, context, lost);
    Logger::instance().flush();
    return result;
  } catch (...) {
    return ERROR_NOT_ENOUGH_MEMORY;
  }
}
