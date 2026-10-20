import Foundation
import ApplicationServices
import CryptoKit
import Darwin

struct ProbeFailure: Error { let message: String; let environment: Bool }
let files = FileManager.default
func ticks() -> UInt64 { mach_absolute_time() }
func now() -> Double { ProcessInfo.processInfo.systemUptime }
func save(_ value: Any, _ url: URL) throws {
    try JSONSerialization.data(withJSONObject: value, options: [.prettyPrinted, .sortedKeys]).write(to: url, options: .atomic)
}
func load(_ url: URL) -> [String: Any]? {
    guard let data = try? Data(contentsOf: url) else { return nil }
    return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
}
func digest(_ url: URL) throws -> String {
    SHA256.hash(data: try Data(contentsOf: url)).map { String(format: "%02x", $0) }.joined()
}
func axResult(_ error: AXError) -> [String: Any] {
    let names: [Int32: String] = [0:"kAXErrorSuccess", -25200:"kAXErrorFailure", -25201:"kAXErrorIllegalArgument",
        -25202:"kAXErrorInvalidUIElement", -25203:"kAXErrorInvalidUIElementObserver", -25204:"kAXErrorCannotComplete",
        -25205:"kAXErrorAttributeUnsupported", -25206:"kAXErrorActionUnsupported", -25207:"kAXErrorNotificationUnsupported",
        -25208:"kAXErrorNotImplemented", -25209:"kAXErrorNotificationAlreadyRegistered", -25210:"kAXErrorNotificationNotRegistered",
        -25211:"kAXErrorAPIDisabled", -25212:"kAXErrorNoValue", -25213:"kAXErrorParameterizedAttributeUnsupported", -25214:"kAXErrorNotEnoughPrecision"]
    return ["numeric": Int(error.rawValue), "symbolic": names[error.rawValue] ?? "unknownAXError"]
}
func optionalAbsence(_ error: AXError) -> Bool { [-25205, -25212].contains(error.rawValue) }
func decoded(_ value: CFTypeRef?) -> Any {
    guard let value else { return NSNull() }
    let type = CFGetTypeID(value)
    if type == CFStringGetTypeID() { return value as! String }
    if type == CFBooleanGetTypeID() { return value as! Bool }
    if type == AXValueGetTypeID() {
        let ax = unsafeBitCast(value, to: AXValue.self)
        if AXValueGetType(ax) == .cgPoint {
            var point = CGPoint.zero
            if AXValueGetValue(ax, .cgPoint, &point) { return ["x":Double(point.x), "y":Double(point.y)] }
        }
        if AXValueGetType(ax) == .cgSize {
            var size = CGSize.zero
            if AXValueGetValue(ax, .cgSize, &size) { return ["width":Double(size.width), "height":Double(size.height)] }
        }
    }
    if type == CFArrayGetTypeID() { return ["type":"array", "count":(value as! [AnyObject]).count] }
    return ["type_id":type, "description":String(describing:value)]
}
final class Inspector {
    let directory: URL
    let pid: pid_t
    var deadline: Double
    var objects: [AXUIElement] = []
    var rows: [[String: Any]] = []
    var trace: FileHandle
    init(directory: URL, pid: pid_t, deadline: Double) throws {
        self.directory = directory; self.pid = pid; self.deadline = deadline
        let url = directory.appendingPathComponent("client-trace.jsonl")
        files.createFile(atPath: url.path, contents: nil)
        trace = try FileHandle(forWritingTo: url)
    }
    func record(_ event: String, _ detail: [String: Any]) {
        let row: [String: Any] = ["event":event, "system_ticks":ticks(), "detail":detail]
        if let bytes = try? JSONSerialization.data(withJSONObject: row, options: [.sortedKeys]) {
            trace.write(bytes); trace.write(Data([10])); trace.synchronizeFile()
        }
    }
    func guardTime() throws {
        if now() >= deadline { throw ProbeFailure(message:"overall discovery/query deadline exceeded", environment:true) }
    }
    func identity(_ element: AXUIElement) -> Int {
        if let index = objects.firstIndex(where: { CFEqual($0, element) }) { return index }
        objects.append(element); return objects.count - 1
    }
    func bind(_ element: AXUIElement) throws {
        try guardTime()
        let timeout = AXUIElementSetMessagingTimeout(element, 2)
        record("messaging_timeout", axResult(timeout))
        guard timeout == .success else { throw ProbeFailure(message:"cannot set finite AX messaging timeout", environment:true) }
        var actual: pid_t = 0
        let result = AXUIElementGetPid(element, &actual)
        record("get_pid", ["result":axResult(result), "actual":actual, "expected":pid])
        guard result == .success && actual == pid else { throw ProbeFailure(message:"public AX object PID mismatch/error", environment:true) }
    }
    func attribute(_ element: AXUIElement, _ name: String, optional: Bool = false) throws -> (CFTypeRef?, [String: Any]) {
        try guardTime()
        var value: CFTypeRef?
        record("attribute_enter", ["object":identity(element), "name":name])
        let result = AXUIElementCopyAttributeValue(element, name as CFString, &value)
        let row: [String: Any] = ["error":axResult(result), "value":decoded(value)]
        record("attribute_return", ["object":identity(element), "name":name, "result":row])
        if result != .success && !(optional && optionalAbsence(result)) {
            throw ProbeFailure(message:"AX attribute \(name) failed with \(result.rawValue)", environment:result.rawValue == -25211 || result.rawValue == -25204)
        }
        if result == .success && value == nil { throw ProbeFailure(message:"AX success with missing value for \(name)", environment:false) }
        return (value, row)
    }
    func actionNames(_ element: AXUIElement) throws -> [String] {
        try guardTime()
        var names: CFArray?
        let result = AXUIElementCopyActionNames(element, &names)
        record("action_names", ["object":identity(element), "error":axResult(result), "names":names as? [String] ?? []])
        guard result == .success, let values = names as? [String] else {
            throw ProbeFailure(message:"public action-name query failed/type mismatch", environment:result.rawValue == -25211 || result.rawValue == -25204)
        }
        guard Set(values).count == values.count else { throw ProbeFailure(message:"duplicate advertised action names", environment:false) }
        return values
    }
    func inspect(_ element: AXUIElement, path: [Int]) throws -> ([String: Any], [AXUIElement]) {
        try bind(element)
        var attrs: [String: Any] = [:]
        for name in ["AXRole", "AXTitle", "AXDescription", "AXIdentifier", "AXEnabled", "AXPosition", "AXSize", "AXValue", "AXChildren"] {
            let optional = ["AXTitle", "AXDescription", "AXIdentifier", "AXEnabled", "AXPosition", "AXSize", "AXValue", "AXChildren"].contains(name)
            attrs[name] = try attribute(element, name, optional: optional).1
        }
        let (rawChildren, childRow) = try attribute(element, "AXChildren", optional: true)
        attrs["AXChildren"] = childRow
        let children: [AXUIElement]
        if let rawChildren {
            guard CFGetTypeID(rawChildren) == CFArrayGetTypeID(), let array = rawChildren as? [AXUIElement],
                array.allSatisfy({CFGetTypeID($0) == AXUIElementGetTypeID()}) else {
                throw ProbeFailure(message:"public children have incompatible type", environment:false)
            }
            children = array
        } else { children = [] }
        let row: [String: Any] = ["object":identity(element), "path":path, "attributes":attrs, "actions":try actionNames(element)]
        return (row, children)
    }
    func walk(_ root: AXUIElement) throws {
        var pending: [(AXUIElement, [Int])] = [(root, [])]
        while let (object, parents) = pending.popLast() {
            try guardTime()
            let id = identity(object)
            guard rows.count < 1024 && parents.count < 64 && !parents.contains(id) else {
                throw ProbeFailure(message:"public AX cycle/depth/node limit", environment:false)
            }
            let path = parents + [id]
            let (row, children) = try inspect(object, path:path)
            rows.append(row)
            try save(rows, directory.appendingPathComponent("public-tree.partial.json"))
            for child in children { pending.append((child, path)) }
        }
    }
}
func value(_ row: [String: Any], _ name: String) -> Any? {
    guard let attrs = row["attributes"] as? [String: Any], let attr = attrs[name] as? [String: Any],
        let error = attr["error"] as? [String: Any], error["numeric"] as? Int == 0 else { return nil }
    return attr["value"]
}
func nonempty(_ value: Any?) -> String? {
    guard let string = value as? String, !string.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
    return string
}
func ownName(_ row: [String: Any]) -> String? { nonempty(value(row,"AXDescription")) ?? nonempty(value(row,"AXTitle")) }
struct MixedStep {
    let phase: String, id: String, positive: Bool
    var listenerAbsent = false, stale = false
    var expectedEnabled: Bool { positive || listenerAbsent || stale }
    var json: [String: Any] { ["phase":phase,"id":id,"positive":positive,"listener_absent":listenerAbsent,"stale":stale] }
}
func mixedPlan(_ mode: String) -> [MixedStep] {
    func s(_ phase: String, _ id: String, _ positive: Bool = true, listener: Bool = false, stale: Bool = false) -> MixedStep {
        MixedStep(phase:phase,id:"mixed."+id,positive:positive,listenerAbsent:listener,stale:stale)
    }
    if mode == "mixed-reload" { return [s("initial","native"),s("initial","builtin"),s("replacement","native"),s("replacement","builtin"),
        s("retired","native",false,stale:true),s("retired","builtin",false,stale:true),s("replacement","native"),s("replacement","builtin")] }
    if mode == "mixed-stop" { return [s("initial","native"),s("initial","builtin"),s("stopped","native",false,stale:true),
        s("stopped","builtin",false,stale:true),s("replacement","native"),s("replacement","builtin")] }
    if mode == "mixed-baseline" { return [s("initial","sibling"),s("initial","native"),s("initial","builtin"),
        s("initial","disabled-native",false),s("initial","disabled-builtin",false),s("initial","sibling")] }
    if mode == "mixed-reactive" { return [s("initial","sibling"),s("updated","sibling"),s("updated","native",false),
        s("updated","builtin",false),s("updated","sibling"),s("enabled","native"),s("enabled","builtin"),
        s("reordered","row-a"),s("conditional-on","conditional"),s("listener-off","builtin",false,listener:true),s("listener-on","builtin"),
        s("assistive-only","native"),s("assistive-only","builtin"),s("long-only","builtin"),s("both-handlers","builtin")] }
    return [s("initial","sibling"),s("initial","native"),s("initial","builtin"),s("intermediate","sibling"),
        s("restored","native"),s("restored","builtin"),s("restored","sibling")]
}
func mixedBudget(_ mode: String) -> Double {
    switch mode { case "mixed-baseline":return 160;case "mixed-reactive":return 440;case "mixed-reload":return 240;default:return 220 }
}
func mixedLedger(_ directory: URL, _ run: String, _ mode: String, _ pid: pid_t) throws -> [String: Any] {
    guard let ledger = load(directory.appendingPathComponent("ledger.json")), ledger["run"] as? String == run,
        ledger["mode"] as? String == mode, ledger["pid"] as? Int == Int(pid), ledger["events"] is [[String: Any]],
        ledger["mixed"] is [String: Any] else { throw ProbeFailure(message:"unbound mixed ledger",environment:true) }
    for event in ledger["events"] as! [[String: Any]] {
        guard event["run"] as? String == run, event["mode"] as? String == mode, event["pid"] as? Int == Int(pid) else {
            throw ProbeFailure(message:"unbound mixed event",environment:true)
        }
    }
    return ledger
}
func mixedEvents(_ ledger: [String: Any]) -> [[String: Any]] { ledger["events"] as! [[String: Any]] }
func mixedEqual(_ left: Any, _ right: Any) -> Bool {
    guard let a = try? JSONSerialization.data(withJSONObject:left,options:[.sortedKeys]),
        let b = try? JSONSerialization.data(withJSONObject:right,options:[.sortedKeys]) else { return false }
    return a == b
}
func mixedControls(_ ledger: [String: Any]) -> [[String: Any]] {
    let mixed = ledger["mixed"] as? [String: Any] ?? [:]
    return Array((mixed["controls"] as? [String: [String: Any]] ?? [:]).values)
}
func mixedGeometry(_ row: [String: Any]) -> Bool {
    guard let p = value(row,"AXPosition") as? [String: Double], let s = value(row,"AXSize") as? [String: Double],
        let x=p["x"], let y=p["y"], let w=s["width"], let h=s["height"] else { return false }
    return [x,y,w,h].allSatisfy { $0.isFinite } && w>0 && h>0
}
func mixedName(_ id: String, _ phase: String) -> String {
    let updated=["updated","enabled","reordered","conditional-off","conditional-on","listener-off","listener-on","assistive-only","long-only","both-handlers"].contains(phase)
    switch id {
    case "mixed.sibling": return "Live sibling"
    case "mixed.native": return updated ? "Updated native" : "Native action"
    case "mixed.builtin": return updated ? "Updated built-in" : "Built-in action"
    case "mixed.disabled-native": return "Disabled native"
    case "mixed.disabled-builtin": return "Disabled built-in"
    case "mixed.conditional": return "Conditional native"
    default: return "Keyed "+id.replacingOccurrences(of:"mixed.",with:"")
    }
}
func mixedRefs(_ ledger: [String: Any]) -> [String: Int] {
    for event in mixedEvents(ledger).reversed() where ["mode_receipt","ready"].contains(event["kind"] as? String ?? "") {
        if let payload=event["payload"] as? [String:Any],let refs=payload["refs"] as? [String:Int] { return refs }
    }
    return [:]
}
func mixedBoundControls(_ ledger: [String: Any], _ id: String, _ ref: Int) -> [[String: Any]] {
    mixedControls(ledger).filter { control in
        let binding=control["binding"] as? [String:Any] ?? [:]
        return control["id"] as? String == id && binding["id"] as? String == id && binding["ref"] as? Int == ref
    }
}
func mixedAlias(_ row: [String: Any], _ id: String, _ phase: String) -> Bool {
    value(row,"AXIdentifier") as? String == id || (value(row,"AXRole") as? String == "AXButton" && ownName(row)==mixedName(id,phase))
}
func mixedAncestors(_ row: [String: Any], _ tree: [[String: Any]]) -> [[String: Any]] {
    guard let path=row["path"] as? [Int] else { return [] }
    return tree.filter{path.dropLast().contains($0["object"] as? Int ?? -1)}
}
func mixedSemanticTree(_ rows: [[String: Any]]) -> [String] {
    rows.map { row in
        let content: [String:Any] = ["attributes":row["attributes"] ?? [:],"actions":row["actions"] ?? []]
        return (try? JSONSerialization.data(withJSONObject:content,options:[.sortedKeys]).base64EncodedString()) ?? "invalid semantic row"
    }.sorted()
}
func runMixedCase(mode: String, target: URL, root: URL, state: URL) -> [String: Any] {
    let run=UUID().uuidString, directory=root.appendingPathComponent(mode), process=Process()
    let deadline=now()+mixedBudget(mode), plan=mixedPlan(mode)
    var result: [String: Any] = ["run":run,"mode":mode,"passed":false,"requests":0,"plan":plan.map{$0.json}]
    var checks: [[String: Any]] = [], operations: [[String: Any]] = [], snapshots: [String: [[String: Any]]] = [:]
    var check: Inspector?, lastLedger: [String: Any]?, transition=0
    func criterion(_ name: String, _ passed: Bool, _ detail: Any) {
        checks.append(["requirement":name,"status":passed ? "verified" : "failed","detail":detail])
    }
    do {
        try files.createDirectory(at:directory,withIntermediateDirectories:true)
        let childState=state.appendingPathComponent(run); try files.createDirectory(at:childState,withIntermediateDirectories:true)
        process.executableURL=target
        var env=ProcessInfo.processInfo.environment
        env["NSH_EXTERNAL_AX_MODE"]=mode; env["NSH_EXTERNAL_AX_NONCE"]=run
        env["NSH_EXTERNAL_AX_DIR"]=directory.path; env["NSH_EXTERNAL_AX_STATE"]=childState.path
        process.environment=env
        for name in ["stdout","stderr"] { files.createFile(atPath:directory.appendingPathComponent("target.\(name)").path,contents:nil) }
        process.standardOutput=try FileHandle(forWritingTo:directory.appendingPathComponent("target.stdout"))
        process.standardError=try FileHandle(forWritingTo:directory.appendingPathComponent("target.stderr"))
        try process.run(); let pid=process.processIdentifier; result["pid"]=pid
        let inspector=try Inspector(directory:directory,pid:pid,deadline:deadline); check=inspector
        let readyDeadline=min(deadline,now()+8)
        while now()<readyDeadline && process.isRunning && load(directory.appendingPathComponent("ready.json")) == nil { Thread.sleep(forTimeInterval:0.02) }
        guard let ready=load(directory.appendingPathComponent("ready.json")),ready["run"] as? String == run,
            ready["mode"] as? String == mode,ready["pid"] as? Int == Int(pid),ready["lifecycle_ready"] as? Bool == true,
            ready["did_finish_launching"] as? Bool == true,ready["is_running"] as? Bool == true,
            ready["action_events_empty"] as? Bool == true else { throw ProbeFailure(message:"mixed lifecycle readiness",environment:true) }
        result["ready"]=ready
        lastLedger=try mixedLedger(directory,run,mode,pid)
        let initialControls=mixedControls(lastLedger!); result["initial_controls"]=initialControls
        func snapshot(_ phase: String) throws {
            inspector.deadline=min(deadline,now()+10)
            inspector.rows=[]
            let app=AXUIElementCreateApplication(pid); try inspector.bind(app)
            let (raw,_)=try inspector.attribute(app,"AXWindows")
            guard let windows=raw as? [AXUIElement],windows.count==1 else { throw ProbeFailure(message:"unique owned window missing",environment:true) }
            let window=windows[0]; try inspector.bind(window)
            let title=try inspector.attribute(window,"AXTitle").0 as? String
            guard title == "External AX \(mode) \(run)" else { throw ProbeFailure(message:"owned window title mismatch",environment:true) }
            try inspector.walk(window)
            snapshots[phase]=inspector.rows
            try save(inspector.rows,directory.appendingPathComponent("public-tree-\(phase).json"))
            lastLedger=try mixedLedger(directory,run,mode,pid)
            try save(lastLedger!,directory.appendingPathComponent("ledger-\(phase).json"))
            let rows=inspector.rows
            let stopped=phase == "stopped", lifecycleCase=["mixed-reload","mixed-stop"].contains(mode)
            let hidden=phase == "intermediate" && mode == "mixed-hidden"
            let merged=phase == "intermediate" && mode == "mixed-merge"
            let core=["mixed.native","mixed.builtin","mixed.disabled-native","mixed.disabled-builtin"]
            for id in ["mixed.sibling"]+core {
                let found=rows.filter{mixedAlias($0,id,phase)}
                let absent=stopped || ((hidden || merged) && id != "mixed.sibling")
                if absent {
                    let aliases=rows.filter{mixedAlias($0,id,phase) || ownName($0)==mixedName(id,phase) || value($0,"AXValue") as? String == mixedName(id,phase)}
                    criterion("\(phase).\(id).absent",aliases.isEmpty,aliases); continue
                }
                let disabled=id.contains("disabled-") || (phase == "updated" && ["mixed.native","mixed.builtin"].contains(id))
                let ok=found.count==1 && ownName(found[0]) == mixedName(id,phase)
                    && value(found[0],"AXRole") as? String == "AXButton" && value(found[0],"AXEnabled") as? Bool == !disabled
                    && mixedGeometry(found[0])
                criterion("\(phase).\(id).semantics",ok,found)
                if !disabled {
                    let count=found.count==1 ? (found[0]["actions"] as? [String] ?? []).filter{$0=="AXPress"}.count : -1
                    let expectedCount=(phase == "listener-off" && id == "mixed.builtin") ? 0:1
                    criterion("\(phase).\(id).advertised_action",count==expectedCount,found)
                }
                if found.count==1 {
                    let ancestors=mixedAncestors(found[0],rows)
                    criterion("\(phase).\(id).no_actionable_ancestor",ancestors.allSatisfy{!($0["actions"] as? [String] ?? []).contains("AXPress")},ancestors)
                }
            }
            let fields=rows.filter{["AXTextField","AXTextArea"].contains(value($0,"AXRole") as? String ?? "")}
            let expectedValue=["updated","enabled","reordered","conditional-off","conditional-on","listener-off","listener-on","assistive-only","long-only","both-handlers"].contains(phase) ? "Updated value" : "Seed value"
            criterion("\(phase).input_semantics_value",(hidden || merged || stopped) ? fields.isEmpty : fields.count==1 && value(fields[0],"AXValue") as? String == expectedValue,fields)
            let texts=rows.filter{value($0,"AXIdentifier") as? String == "mixed.text" || ownName($0)=="Mixed text"}
            criterion("\(phase).mixed_text_presence",(hidden || merged || stopped) ? texts.isEmpty : texts.count==1,texts)
            if hidden || merged {
                let valueAliases=rows.filter{["Mixed input","Mixed text","Seed value","Unrelated initial text"].contains(ownName($0) ?? "")
                    || ["Seed value","Unrelated initial text"].contains(value($0,"AXValue") as? String ?? "")}
                criterion("\(phase).no_mixed_semantic_aliases",valueAliases.isEmpty,valueAliases)
            }
            let group=rows.filter{value($0,"AXIdentifier") as? String == "mixed.group"}
            if hidden { criterion("\(phase).group_hidden",group.isEmpty,group) }
            if merged { criterion("\(phase).group_merged",group.count==1 && value(group[0],"AXRole") as? String == "AXGroup",group) }
            let initial=initialControls.filter{$0["id"] as? String != "mixed.conditional"}.sorted{($0["id"] as? String ?? "")<($1["id"] as? String ?? "")}
            let current=mixedControls(lastLedger!).filter{$0["id"] as? String != "mixed.conditional"}.sorted{($0["id"] as? String ?? "")<($1["id"] as? String ?? "")}
            if lifecycleCase && phase != "initial" {
                let recreated=current.count==initial.count && !current.isEmpty && current.allSatisfy{row in !initial.contains{$0["mount"] as? Int == row["mount"] as? Int}}
                criterion(phase+".lifecycle_mounts",stopped ? current.isEmpty:recreated,["initial":initial,"current":current])
            } else {
                criterion("\(phase).retained_native_mounts",initial.map{$0["mount"] as? Int} == current.map{$0["mount"] as? Int}
                    && initial.map{$0["object"] as? Int} == current.map{$0["object"] as? Int},["initial":initial,"current":current])
            }
            if ["unrelated","restored","listener-off","listener-on","assistive-only","long-only","both-handlers"].contains(phase) {
                let first=snapshots["initial"] ?? []
                for id in ["mixed.native","mixed.builtin","mixed.sibling"] {
                    let a=first.filter{value($0,"AXIdentifier") as? String == id},b=rows.filter{value($0,"AXIdentifier") as? String == id}
                    let equal=a.count==1 && b.count==1 && a[0]["object"] as? Int == b[0]["object"] as? Int
                    criterion("\(phase).\(id).public_identity",phase == "restored" || equal,["initial":a,"current":b,"equal_diagnostic":equal,"required_only_for_unrelated_update":phase == "unrelated"])
                }
                let referencePhase=["listener-off","listener-on","assistive-only","long-only","both-handlers"].contains(phase) ? "conditional-on":"initial"
                let baseline=referencePhase == "initial" ? result["initial_ledger"] as? [String:Any] : load(directory.appendingPathComponent("ledger-conditional-on.json"))
                guard let baseline else {throw ProbeFailure(message:"missing retained-ref baseline after intended recreation",environment:true)}
                let initialRefs=mixedRefs(baseline), currentRefs=mixedRefs(lastLedger!)
                criterion(phase+".retained_js_refs",!initialRefs.isEmpty && initialRefs==currentRefs,["baseline_phase":referencePhase,"initial":initialRefs,"current":currentRefs])
                if phase == "restored" {
                    criterion("restored.semantic_tree",mixedSemanticTree(first)==mixedSemanticTree(rows),["initial":first,"restored":rows])
                }
            }
            if phase == "reordered" {
                let keyed=rows.filter{["mixed.row-a","mixed.row-b"].contains(value($0,"AXIdentifier") as? String ?? "")}
                criterion("reordered.keyed_presence",keyed.count==2,keyed)
                if keyed.count==2,let a=keyed.first(where:{value($0,"AXIdentifier") as? String == "mixed.row-a"}),let b=keyed.first(where:{value($0,"AXIdentifier") as? String == "mixed.row-b"}),
                    let ap=value(a,"AXPosition") as? [String:Double],let bp=value(b,"AXPosition") as? [String:Double] {
                    criterion("reordered.keyed_order",(bp["y"] ?? .infinity)<(ap["y"] ?? -.infinity),keyed)
                }
            }
            if phase == "conditional-off" || phase == "conditional-on" {
                let old=initialControls.filter{$0["id"] as? String == "mixed.conditional"}
                let initialRefs=mixedRefs(result["initial_ledger"] as? [String:Any] ?? lastLedger!)
                let refs=mixedRefs(lastLedger!), currentRef=refs["mixed.conditional"] ?? -1
                let current=mixedBoundControls(lastLedger!,"mixed.conditional",currentRef)
                let allInstances=mixedControls(lastLedger!).filter{$0["id"] as? String == "mixed.conditional"}
                let publicRows=rows.filter{mixedAlias($0,"mixed.conditional",phase)}
                let receipt=mixedEvents(lastLedger!).last(where:{$0["kind"] as? String == "mode_receipt"})?["payload"] as? [String:Any] ?? [:]
                let state=receipt["js_state"] as? [String:Any] ?? [:]
                let removed=publicRows.isEmpty && state["present"] as? Bool == false
                let recreated=current.count==1 && publicRows.count==1 && old.count==1
                    && state["present"] as? Bool == true && currentRef != initialRefs["mixed.conditional"]
                    && current[0]["mount"] as? Int != old[0]["mount"] as? Int
                // Detached nodes may await GC. Public presence and current ref
                // binding define the live instance; destruction timing does not.
                criterion(phase+".conditional_lifecycle",phase == "conditional-off" ? removed:recreated,
                    ["old":old,"current":current,"retained_instances_diagnostic":allInstances,"public":publicRows,"current_js_ref":currentRef])
            }
        }
        func move(_ phase: String) throws {
            transition += 1
            let payload: [String: Any] = ["run":run,"case":mode,"transition":transition,"phase":phase]
            let before=try mixedLedger(directory,run,mode,pid), count=mixedEvents(before).count
            inspector.record("transition_request",payload)
            try save(payload,directory.appendingPathComponent("transition.json"))
            let until=min(deadline,now()+4)
            var received: [[String:Any]]=[]
            while now()<until && process.isRunning {
                lastLedger=try mixedLedger(directory,run,mode,pid)
                received=mixedEvents(lastLedger!).filter{$0["kind"] as? String == "mode_receipt" && ($0["payload"] as? [String:Any])?["transition"] as? Int == transition}
                if received.count==1 { break }; Thread.sleep(forTimeInterval:0.02)
            }
            guard received.count==1,let actual=received[0]["payload"] as? [String:Any], actual["run"] as? String == run,
                actual["case"] as? String == mode,actual["phase"] as? String == phase else { throw ProbeFailure(message:"missing/mismatched JS mode receipt",environment:true) }
            let route=Array(mixedEvents(lastLedger!).dropFirst(count)).filter{["control_emit","mode_receipt"].contains($0["kind"] as? String ?? "")}
            guard route.count==2,route[0]["kind"] as? String == "control_emit",route[1]["kind"] as? String == "mode_receipt" else { throw ProbeFailure(message:"transition route not exactly component to JS to host",environment:true) }
            inspector.record("transition_receipt",["route":route])
            Thread.sleep(forTimeInterval:0.35)
            try snapshot(phase)
        }
        func act(_ step: MixedStep, _ index: Int) throws {
            var operation=step.json; operation["step"]=index;operation["query_enter_ticks"]=ticks()
            inspector.deadline=min(deadline,now()+10)
            inspector.rows=[]
            let currentApp=AXUIElementCreateApplication(pid);try inspector.bind(currentApp)
            let (rawWindows,_)=try inspector.attribute(currentApp,"AXWindows")
            guard let windows=rawWindows as? [AXUIElement],windows.count==1 else {
                throw ProbeFailure(message:"preaction unique owned window missing",environment:true)
            }
            try inspector.bind(windows[0])
            guard try inspector.attribute(windows[0],"AXTitle").0 as? String == "External AX \(mode) \(run)" else {
                throw ProbeFailure(message:"preaction owned window mismatch",environment:true)
            }
            try inspector.walk(windows[0])
            let tree=inspector.rows
            try save(tree,directory.appendingPathComponent("public-tree-preaction-\(index).json"))
            let found=tree.filter{mixedAlias($0,step.id,step.phase)}
            guard found.count==1,let object=found[0]["object"] as? Int,object<inspector.objects.count else {
                operation["status"]="unexecuted_product_discovery_failure"; operations.append(operation); return
            }
            // Tree traversal and inspection share the same ten-second scope.
            let element=inspector.objects[object]
            let row=try inspector.inspect(element,path:found[0]["path"] as? [Int] ?? []).0
            try inspector.guardTime();operation["query_return_ticks"]=ticks()
            operation["pre_action_control"]=row
            let ancestors=mixedAncestors(row,tree)
            let ancestryOK=(row["path"] as? [Int])?.last==object
                && ancestors.allSatisfy{!($0["actions"] as? [String] ?? []).contains("AXPress")}
            let advertised=(row["actions"] as? [String] ?? []).filter{$0=="AXPress"}.count==1
            let builtin=step.id.contains("builtin")
            operation["advertised_action"]=advertised;operation["ancestors"]=ancestors
            operation["intentional_unadvertised_builtin_request"]=builtin && !advertised
            // The explicit built-in request diagnoses its external action route.
            // Missing advertisement remains a failed requirement, never a pass.
            let eligible=ownName(row)==mixedName(step.id,step.phase) && value(row,"AXRole") as? String == "AXButton"
                && value(row,"AXIdentifier") as? String == step.id && value(row,"AXEnabled") as? Bool == step.expectedEnabled
                && mixedGeometry(row) && ancestryOK && (!step.positive || advertised || builtin)
            guard eligible else { operation["status"]="unexecuted_product_state_or_alias_failure"; operations.append(operation); return }
            let before=try mixedLedger(directory,run,mode,pid), previous=mixedEvents(before)
            let refs=mixedRefs(before)
            guard let expectedRef=refs[step.id] else { throw ProbeFailure(message:"missing bound JS ref for action",environment:true) }
            operation["expected_js_ref"]=expectedRef
            let native = !builtin
            let nativeControl=mixedBoundControls(before,step.id,expectedRef)
            if native {
                guard nativeControl.count==1,let mount=nativeControl[0]["mount"] as? Int else {
                    throw ProbeFailure(message:"native action cannot bind current JS ref to owned mount",environment:true)
                }
                let bindings=previous.filter{ event in
                    let payload=event["payload"] as? [String:Any] ?? [:]
                    return event["kind"] as? String == "ref_bound" && event["mount"] as? Int == mount
                        && payload["id"] as? String == step.id && payload["ref"] as? Int == expectedRef
                }
                guard bindings.count==1 else { throw ProbeFailure(message:"native current-ref binding receipt mismatch",environment:true) }
                operation["current_binding_receipt"]=bindings[0]
            }
            operation["before_ledger"]=before; inspector.deadline=deadline
            operation["request_enter_ticks"]=ticks(); inspector.record("mixed_request_enter",operation)
            let error=AXUIElementPerformAction(element,"AXPress" as CFString)
            operation["request_return_ticks"]=ticks(); operation["action_error"]=axResult(error)
            inspector.record("mixed_request_return",operation)
            result["requests"]=(result["requests"] as? Int ?? 0)+1
            let detachedMount=initialControls.first(where:{$0["id"] as? String == "mixed.conditional"})?["mount"] as? Int
            func detachedDrop(_ event: [String:Any]) -> Bool {
                let control=event["control"] as? [String:Any] ?? [:]
                return step.phase == "conditional-on" && step.id == "mixed.conditional"
                    && event["kind"] as? String == "native_drop" && detachedMount != nil
                    && event["mount"] as? Int == detachedMount && control["id"] as? String == "mixed.conditional"
                    && nativeControl.first?["mount"] as? Int != detachedMount
            }
            let start=now(); var lastChange=start, latest=previous
            var lastObserved=previous.filter{!detachedDrop($0)}
            while now()<min(deadline,start+4.35) && process.isRunning {
                lastLedger=try mixedLedger(directory,run,mode,pid)
                let events=mixedEvents(lastLedger!), observedEvents=events.filter{!detachedDrop($0)}
                if !mixedEqual(observedEvents,lastObserved) { lastObserved=observedEvents;lastChange=now() }
                latest=events
                Thread.sleep(forTimeInterval:0.02)
            }
            let prefix=latest.count>=previous.count && mixedEqual(Array(latest.prefix(previous.count)),previous)
            guard prefix else { throw ProbeFailure(message:"action ledger history mutated",environment:true) }
            let rawEffects=Array(latest.dropFirst(previous.count)), effects=rawEffects.filter{!detachedDrop($0)}
            let observed=now()-start>=4 && now()-lastChange>=0.35 && now()<deadline && process.isRunning
            // Preserve all raw events. Collection of the known detached initial
            // conditional instance cannot invalidate current-instance delivery.
            operation["raw_effects"]=rawEffects;operation["detached_lifetime_diagnostics"]=rawEffects.filter{detachedDrop($0)}
            operation["effects"]=effects;operation["observed_seconds"]=now()-start;operation["quiet_seconds"]=now()-lastChange
            operation["observation_end_ticks"]=ticks();operation["bounded_observation_passed"]=observed
            operation["after_ledger"]=lastLedger!
            let callbacks=effects.filter{$0["kind"] as? String == "callback"}
            let oldCallbacks=previous.filter{$0["kind"] as? String == "callback" && $0["generation"] as? Int == before["generation"] as? Int
                && ($0["payload"] as? [String:Any])?["id"] as? String == step.id}.count
            let payload=callbacks.first?["payload"] as? [String:Any] ?? [:]
            let kinds=effects.map{$0["kind"] as? String ?? ""}
            let nativeSequence=(nativeControl.first?["sequence"] as? Int ?? 0)+1
            let raw=payload["raw"] as? [String:Any] ?? [:]
            let nativePayload=effects.first?["payload"] as? [String:Any] ?? [:]
            let pressPayload=effects.dropFirst().first?["payload"] as? [String:Any] ?? [:]
            let mount=nativeControl.first?["mount"] as? Int ?? -1
            let expectedRaw: [String:Any] = ["id":step.id,"appearance":"native","mount":mount,"phase":step.phase,"sequence":nativeSequence]
            var expectedEntry=expectedRaw;expectedEntry["sequence"]=nativeSequence-1
            let nativeExact=native && nativeControl.count==1 && mixedEqual(nativePayload,expectedEntry)
                && mixedEqual(pressPayload,expectedRaw) && mixedEqual(raw,expectedRaw)
            let facade=effects.first(where:{$0["kind"] as? String == "facade_entry"})?["payload"] as? [String:Any] ?? [:]
            let expectedFacade: [String:Any] = ["id":step.id,"appearance":native ? "native":"custom","phase":step.phase,"enabled":true,"ref":expectedRef]
            let customRaw: Any = step.phase == "long-only" ? ["source":"longPress"]:NSNull()
            let expectedCallback: [String:Any] = ["id":step.id,"appearance":native ? "native":"custom","phase":step.phase,
                "sequence":oldCallbacks+1,"ref":expectedRef,"raw":native ? expectedRaw:customRaw]
            let exact=callbacks.count==1 && mixedEqual(payload,expectedCallback) && mixedEqual(facade,expectedFacade)
                && (native ? kinds == ["target_entry","native_press","facade_entry","callback"] && nativeExact
                    : kinds == ["facade_entry","callback"])
            let classifiedAPI=error == .success || error == .actionUnsupported
            operation["api_classified"]=classifiedAPI
            if step.positive {
                operation["delivery_exact"]=exact
                operation["status"]=(error == .success && exact && observed && advertised) ? "verified":"failed_external_activation_or_advertisement"
            } else {
                operation["strict_refusal"]=false;operation["disabled_behavior_passed"]=false
                operation["classification"] = effects.isEmpty && observed && (error == .success || error == .actionUnsupported)
                    ? (error == .success ? "disabled_suppressed_api_success":"disabled_suppressed_action_unsupported") : "disabled_failed_or_inconclusive"
                operation["status"]=effects.isEmpty && observed ? "provisional_disabled_no_effect":"failed_disabled_effect_or_observation"
                operation["preguard_observable"]=true
                operation["entry_observation_scope"]=native ? "native target before enabled guard":"framework-to-application facade before fixture guard"
            }
            operations.append(operation)
            try save(operations,directory.appendingPathComponent("operations.partial.json"))
            if !classifiedAPI {
                throw ProbeFailure(message:"unclassified AX action result \(error.rawValue); stop as oracle/transport inconclusive",environment:error == .cannotComplete || error == .apiDisabled)
            }
            if !observed { throw ProbeFailure(message:"bounded action observation incomplete",environment:true) }
        }
        func lifecycle(_ operation: String, _ sequence: Int) throws {
            let payload: [String:Any] = ["run":run,"case":mode,"sequence":sequence,"operation":operation]
            inspector.record("host_lifecycle_request",payload)
            try save(payload,directory.appendingPathComponent("lifecycle.json"))
            let name=operation == "stop" ? "stopped":"replacement"
            let until=min(deadline,now()+4)
            var receipt: [String:Any]?
            while now()<until && process.isRunning {
                if let candidate=load(directory.appendingPathComponent("lifecycle-\(name).json")),candidate["sequence"] as? Int == sequence {receipt=candidate;break}
                Thread.sleep(forTimeInterval:0.02)
            }
            guard let receipt,receipt["run"] as? String == run,receipt["mode"] as? String == mode,
                receipt["retained_public_controls"] as? Int == 2,receipt["generation"] as? Int == (operation == "stop" ? 1:2) else {
                throw ProbeFailure(message:"bound public AppView lifecycle receipt missing",environment:true)
            }
            inspector.record("host_lifecycle_receipt",receipt)
            result["lifecycle_\(sequence)"]=receipt
            Thread.sleep(forTimeInterval:0.35)
            try snapshot(name)
        }
        var oldHandles: [String:(AXUIElement,[String:Any])] = [:]
        func retainHandles() throws {
            for id in ["mixed.native","mixed.builtin"] {
                let rows=(snapshots["initial"] ?? []).filter{mixedAlias($0,id,"initial")}
                guard rows.count==1,let object=rows[0]["object"] as? Int,object<inspector.objects.count,
                    value(rows[0],"AXEnabled") as? Bool == true,
                    (rows[0]["actions"] as? [String] ?? []).contains("AXPress") else {
                    throw ProbeFailure(message:"stale case requires validated initially live public peers",environment:false)
                }
                oldHandles[id]=(inspector.objects[object],rows[0])
            }
            result["retained_public_handles"]=oldHandles.mapValues{$0.1}
        }
        func staleAction(_ step: MixedStep, _ index: Int) throws {
            guard let (element,initialRow)=oldHandles[step.id] else {throw ProbeFailure(message:"missing declared old public handle",environment:true)}
            let before=try mixedLedger(directory,run,mode,pid),previous=mixedEvents(before)
            var operation=step.json;operation["step"]=index;operation["cached_initial_public_control"]=initialRow
            operation["before_ledger"]=before
            operation["request_enter_ticks"]=ticks();inspector.record("stale_request_enter",operation)
            let error=AXUIElementPerformAction(element,"AXPress" as CFString)
            operation["request_return_ticks"]=ticks();operation["action_error"]=axResult(error)
            inspector.record("stale_request_return",operation)
            result["requests"]=(result["requests"] as? Int ?? 0)+1
            let start=now();var latest=previous,lastChange=start
            while now()<min(deadline,start+4.35) && process.isRunning {
                lastLedger=try mixedLedger(directory,run,mode,pid)
                let events=mixedEvents(lastLedger!)
                if !mixedEqual(events,latest) {latest=events;lastChange=now()}
                Thread.sleep(forTimeInterval:0.02)
            }
            guard latest.count>=previous.count && mixedEqual(Array(latest.prefix(previous.count)),previous) else {
                throw ProbeFailure(message:"retired-handle ledger prefix changed",environment:true)
            }
            let effects=Array(latest.dropFirst(previous.count))
            let observed=now()-start>=4 && now()-lastChange>=0.35 && now()<deadline && process.isRunning
            let known=error == .success || error == .actionUnsupported || error == .invalidUIElement
            operation["api_classified"]=known;operation["effects"]=effects;operation["raw_effects"]=effects
            operation["observed_seconds"]=now()-start;operation["quiet_seconds"]=now()-lastChange
            operation["observation_end_ticks"]=ticks();operation["bounded_observation_passed"]=observed
            operation["after_ledger"]=lastLedger!;operation["strict_refusal"]=false
            operation["status"]=known && effects.isEmpty && observed ? "provisional_retired_no_effect":"failed_or_inconclusive_retired_request"
            operations.append(operation);try save(operations,directory.appendingPathComponent("operations.partial.json"))
            if !known || !observed {throw ProbeFailure(message:"retired request has unclassified API result or incomplete observation",environment:!observed || error == .cannotComplete || error == .apiDisabled)}
        }
        try snapshot("initial")
        result["initial_ledger"]=lastLedger!
        if ["mixed-reload","mixed-stop"].contains(mode) {
            try retainHandles()
            for i in 0..<2 {try act(plan[i],i+1)}
            guard operations.count==2 && operations.allSatisfy({$0["status"] as? String == "verified"}) else {
                throw ProbeFailure(message:"retired-handle case needs working initial peer actions",environment:false)
            }
            if mode == "mixed-reload" {
                try lifecycle("reload",1)
                for i in 2..<4 {try act(plan[i],i+1)}
                for i in 4..<6 {try staleAction(plan[i],i+1)}
                for i in 6..<8 {try act(plan[i],i+1)}
            } else {
                try lifecycle("stop",1)
                for i in 2..<4 {try staleAction(plan[i],i+1)}
                try lifecycle("restart",2)
                for i in 4..<6 {try act(plan[i],i+1)}
            }
        } else {
            var index=0
            let phases=mode == "mixed-baseline" ? ["initial"] : mode == "mixed-reactive"
                ? ["initial","unrelated","updated","enabled","reordered","conditional-off","conditional-on","listener-off","listener-on","assistive-only","long-only","both-handlers"] : ["initial","intermediate","restored"]
            for phase in phases {
                if phase != "initial" { try move(phase) }
                for step in plan where step.phase==phase {index+=1;try act(step,index)}
            }
        }
        result["status"]="completed_with_requirement_results"
    } catch let failure as ProbeFailure { result["status"]="stopped_oracle_or_environment"; result["error"]=failure.message; result["environment_blocked"]=failure.environment; result["stop_suite"]=true }
      catch { result["status"]="stopped_oracle_or_environment";result["error"]=String(describing:error);result["stop_suite"]=true }
    if process.processIdentifier>0 {
        try? save(["run":run,"mode":mode],directory.appendingPathComponent("stop.json"))
        let until=now()+3
        while process.isRunning && now()<until { Thread.sleep(forTimeInterval:0.02) }
        if process.isRunning { process.terminate();let until=now()+1;while process.isRunning && now()<until {Thread.sleep(forTimeInterval:0.02)} }
        if process.isRunning { kill(process.processIdentifier,SIGKILL) }
        result["survivor"]=process.isRunning
        let final=load(directory.appendingPathComponent("final.json"));result["final"]=final ?? [:]
        let finalLedger=final?["ledger"] as? [String:Any]
        let finalEvents=finalLedger?["events"] as? [[String:Any]] ?? []
        let late=Array(finalEvents.filter{["target_entry","native_press","facade_entry","callback"].contains($0["kind"] as? String ?? "")})
        let observed=(lastLedger?["events"] as? [[String:Any]] ?? []).filter{["target_entry","native_press","facade_entry","callback"].contains($0["kind"] as? String ?? "")}
        let expectedActions=operations.filter{$0["positive"] as? Bool == true}.flatMap{$0["effects"] as? [[String:Any]] ?? []}
            .filter{["target_entry","native_press","facade_entry","callback"].contains($0["kind"] as? String ?? "")}
        let clean = !process.isRunning && process.terminationStatus==0 && final?["run"] as? String == run
            && final?["mode"] as? String == mode && final?["graceful"] as? Bool == true
            && final?["window_visible"] as? Bool == false && final?["content_detached"] as? Bool == true
            && final?["is_running_after_run"] as? Bool == false && final?["stop_called"] as? Bool == true
            && (final?["heartbeat"] as? Int ?? 0)>((result["ready"] as? [String:Any])?["heartbeat"] as? Int ?? 0)
            && finalLedger != nil && mixedEqual(late,observed) && mixedEqual(late,expectedActions)
        criterion("cleanup_and_final_action_ledger",clean,["late_actions":late,"observed_actions":observed,"expected_actions":expectedActions])
        if !clean {result["stop_suite"]=true}
        for i in operations.indices where operations[i]["positive"] as? Bool == false {
            let operation=operations[i],id=operation["id"] as? String ?? "",phase=operation["phase"] as? String ?? ""
            let stale=operation["stale"] as? Bool == true,listener=operation["listener_absent"] as? Bool == true
            let native = !id.contains("builtin")
            let before=operations[..<i].last(where:{$0["positive"] as? Bool == true && $0["status"] as? String == "verified" && (!(stale || listener) || $0["id"] as? String == id)})
            let after=operations.dropFirst(i+1).first(where:{$0["positive"] as? Bool == true && $0["status"] as? String == "verified" && (!(stale || listener) || $0["id"] as? String == id)})
            let samePhase=before?["phase"] as? String == phase && after?["phase"] as? String == phase
            let successfulAppearance=operations.contains{($0["id"] as? String ?? "").contains("builtin") == !native && $0["positive"] as? Bool == true && $0["status"] as? String == "verified"}
            let zero=finalEvents.filter{["target_entry","native_press","facade_entry","callback"].contains($0["kind"] as? String ?? "") && ($0["payload"] as? [String:Any])?["id"] as? String == id && ($0["payload"] as? [String:Any])?["phase"] as? String == phase}.isEmpty
            let numeric=(operation["action_error"] as? [String:Any])?["numeric"] as? Int
            let bracketed=(stale || listener) ? before != nil && after != nil:samePhase
            let acceptedErrors=stale ? [0,-25206,-25202]:[0,-25206]
            let status=operation["status"] as? String ?? ""
            let provisional=status == "provisional_disabled_no_effect" || status == "provisional_retired_no_effect"
            let observedEntry=stale || operation["preguard_observable"] as? Bool == true
            let qualified=clean && bracketed && successfulAppearance && observedEntry && zero
                && operation["bounded_observation_passed"] as? Bool == true && acceptedErrors.contains(numeric ?? 999) && provisional
            operations[i][stale ? "retired_behavior_passed":listener ? "listener_refusal_passed":"disabled_behavior_passed"]=qualified
            operations[i]["strict_refusal"]=qualified && numeric != 0
            if provisional {operations[i]["status"]=qualified ? "verified":"inconclusive_negative_observability_or_live_controls"}
            operations[i]["live_before"]=before?["step"] ?? NSNull();operations[i]["live_after"]=after?["step"] ?? NSNull()
            operations[i]["same_phase_brackets"]=samePhase;operations[i]["successful_appearance"]=successfulAppearance
        }
    }
    result["checks"]=checks;result["operations"]=operations
    result["passed"]=result["stop_suite"] as? Bool != true && checks.allSatisfy{$0["status"] as? String == "verified"} && operations.count==plan.count && operations.allSatisfy{$0["status"] as? String == "verified"}
    result["unexecuted_plan_count"]=plan.count-operations.filter{$0["request_return_ticks"] != nil}.count
    check?.trace.closeFile();try? save(result,directory.appendingPathComponent("case-result.json"))
    return result
}


let args=CommandLine.arguments
let allModes=["mixed-baseline","mixed-merge","mixed-hidden","mixed-reactive","mixed-reload","mixed-stop"]
let filtered=args.count==6 && args[4]=="--case" && allModes.contains(args[5])
guard args.count==4 || filtered else {
    fputs("usage: client TARGET_EXECUTABLE OUTPUT_DIRECTORY STATE_DIRECTORY [--case CASE]\n",stderr);exit(64)
}
let modes=filtered ? [args[5]]:allModes
let target=URL(fileURLWithPath:args[1]), output=URL(fileURLWithPath:args[2]), state=URL(fileURLWithPath:args[3])
do {
    guard !files.fileExists(atPath:output.appendingPathComponent("client-summary.json").path),
        modes.allSatisfy({!files.fileExists(atPath:output.appendingPathComponent($0).path)}) else {
        throw ProbeFailure(message:"refusing to overwrite earlier regression evidence",environment:true)
    }
    try files.createDirectory(at:output,withIntermediateDirectories:true)
    let client=URL(fileURLWithPath:args[0]).standardizedFileURL
    var timebase=mach_timebase_info_data_t();mach_timebase_info(&timebase)
    let trusted=AXIsProcessTrusted() // No prompt, before any target exists.
    let budget=modes.reduce(20.0){$0+mixedBudget($1)}
    var summary: [String:Any] = ["schema":1,"candidate_base":"0cad7607048909e97129c67cd5f384be5b9b2193",
        "candidate_is_unmodified_base":false,"source_identity":"declared isolated workspace; exact source manifest in orchestration evidence",
        "client":["path":client.path,"sha256":try digest(client),"pid":getpid()],
        "target":["path":target.path,"sha256":try digest(target)],"trusted":trusted,"trust_prompt":false,
        "clock":"mach_absolute_time","timebase_numer":timebase.numer,"timebase_denom":timebase.denom,
        "trust_system_ticks":ticks(),"planned_cases":modes.map{["mode":$0,"actions":mixedPlan($0).map{$0.json},"budget_seconds":mixedBudget($0)]},
        "suite_budget_seconds":budget,"per_ax_call_seconds":2,"targets_launched":0,"requests":0,"cases":[],"passed":false]
    try save(summary,output.appendingPathComponent("client-summary.json"))
    guard trusted else {
        summary["environment_blocked"]=true;summary["reason"]="Actual SDK client is not trusted; no prompt, target or action requested."
        try save(summary,output.appendingPathComponent("client-summary.json"));exit(77)
    }
    let deadline=now()+budget
    var cases: [[String:Any]]=[]
    for mode in modes {
        if now()>=deadline {summary["suite_deadline_exceeded"]=true;break}
        let result=runMixedCase(mode:mode,target:target,root:output,state:state)
        cases.append(result);summary["cases"]=cases
        summary["targets_launched"]=cases.filter{$0["pid"] != nil}.count
        summary["requests"]=cases.reduce(0){$0+($1["requests"] as? Int ?? 0)}
        try save(summary,output.appendingPathComponent("client-summary.json"))
        if result["stop_suite"] as? Bool == true || result["survivor"] as? Bool == true {break}
    }
    for mode in modes where !cases.contains(where:{$0["mode"] as? String == mode}) {
        cases.append(["mode":mode,"status":"unexecuted_oracle_or_environment_stop","passed":false,"requests":0,"plan":mixedPlan(mode).map{$0.json}])
    }
    let passed=cases.count==modes.count && cases.allSatisfy{$0["passed"] as? Bool == true}
    summary["cases"]=cases;summary["passed"]=passed
    summary["limits"]="Dependency candidate only. Public live discovery; cached old handles only in declared reload/stop steps. API status and effect independent; no final-app, signing, keyboard/IME or CU certification."
    try save(summary,output.appendingPathComponent("client-summary.json"));exit(passed ? 0:1)
} catch { fputs("AX regression client error: \(error)\n",stderr);exit(1) }
