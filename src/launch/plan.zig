const std = @import("std");
const install_plan = @import("../install/plan.zig");
const java_runtime = @import("../java/java_runtime.zig");
const metadata = @import("../install/version_metadata.zig");

pub const Auth = struct {
    player_name: []const u8,
    uuid: []const u8,
    access_token: []const u8,
    session: ?[]const u8 = null,
    user_type: []const u8 = "msa",
    xuid: []const u8 = "",
    client_id: []const u8 = "",
    user_properties: []const u8 = "{}",
};

pub const Options = struct {
    demo: bool = false,
    resolution_width: ?u32 = null,
    resolution_height: ?u32 = null,
    quick_play_support: bool = false,
    quick_play_path: []const u8 = "",
    quick_play_singleplayer: []const u8 = "",
    quick_play_multiplayer: []const u8 = "",
    quick_play_realms: []const u8 = "",
    launcher_name: []const u8 = "jmcl-core",
    launcher_version: []const u8 = "0.1.0-dev",
    include_default_user_jvm: bool = true,
    extra_jvm_args: []const []const u8 = &.{},
    extra_game_args: []const []const u8 = &.{},
};

pub const Input = struct {
    metadata: metadata.Metadata,
    install: install_plan.Data,
    runtime: java_runtime.Runtime,
    root_directory: []const u8,
    game_directory: []const u8,
    natives_directory: []const u8,
    auth: Auth,
    options: Options = .{},
    force_java: bool = false,
};

pub const Data = struct {
    java_executable: []const u8,
    working_directory: []const u8,
    jvm_args: []const []const u8,
    main_class: []const u8,
    game_args: []const []const u8,
    argv: []const []const u8,
    redacted_argv: []const []const u8,
    sensitive_argv_indices: []const usize,
};

pub const Plan = struct {
    arena: *std.heap.ArenaAllocator,
    value: Data,

    pub fn deinit(plan: *Plan) void {
        const allocator = plan.arena.child_allocator;
        plan.arena.deinit();
        allocator.destroy(plan.arena);
        plan.* = undefined;
    }
};

const Resolver = struct {
    input: Input,
    allocator: std.mem.Allocator,
    assets_root: []const u8,
    asset_index_name: []const u8,
    game_assets: []const u8,
    library_directory: []const u8,
    classpath: []const u8,
    classpath_separator: []const u8,
    resolution_width: []const u8,
    resolution_height: []const u8,

    fn value(resolver: Resolver, name: []const u8) ?[]const u8 {
        const auth = resolver.input.auth;
        const options = resolver.input.options;
        if (std.mem.eql(u8, name, "auth_player_name")) return auth.player_name;
        if (std.mem.eql(u8, name, "auth_uuid")) return auth.uuid;
        if (std.mem.eql(u8, name, "auth_access_token")) return auth.access_token;
        if (std.mem.eql(u8, name, "auth_session")) return auth.session orelse auth.access_token;
        if (std.mem.eql(u8, name, "user_type")) return auth.user_type;
        if (std.mem.eql(u8, name, "auth_xuid")) return auth.xuid;
        if (std.mem.eql(u8, name, "clientid")) return auth.client_id;
        if (std.mem.eql(u8, name, "user_properties")) return auth.user_properties;
        if (std.mem.eql(u8, name, "version_name")) return resolver.input.metadata.id;
        if (std.mem.eql(u8, name, "version_type")) return resolver.input.metadata.type;
        if (std.mem.eql(u8, name, "game_directory")) return resolver.input.game_directory;
        if (std.mem.eql(u8, name, "assets_root")) return resolver.assets_root;
        if (std.mem.eql(u8, name, "assets_index_name")) return resolver.asset_index_name;
        if (std.mem.eql(u8, name, "game_assets")) return resolver.game_assets;
        if (std.mem.eql(u8, name, "natives_directory")) return resolver.input.natives_directory;
        if (std.mem.eql(u8, name, "library_directory")) return resolver.library_directory;
        if (std.mem.eql(u8, name, "classpath")) return resolver.classpath;
        if (std.mem.eql(u8, name, "classpath_separator")) return resolver.classpath_separator;
        if (std.mem.eql(u8, name, "launcher_name")) return options.launcher_name;
        if (std.mem.eql(u8, name, "launcher_version")) return options.launcher_version;
        if (std.mem.eql(u8, name, "resolution_width")) return resolver.resolution_width;
        if (std.mem.eql(u8, name, "resolution_height")) return resolver.resolution_height;
        if (std.mem.eql(u8, name, "quickPlayPath")) return options.quick_play_path;
        if (std.mem.eql(u8, name, "quickPlaySingleplayer")) return options.quick_play_singleplayer;
        if (std.mem.eql(u8, name, "quickPlayMultiplayer")) return options.quick_play_multiplayer;
        if (std.mem.eql(u8, name, "quickPlayRealms")) return options.quick_play_realms;
        return null;
    }
};

pub fn build(allocator: std.mem.Allocator, input: Input) !Plan {
    if (input.auth.player_name.len == 0 or input.auth.uuid.len == 0 or input.auth.access_token.len == 0) {
        return error.InvalidAuthSession;
    }
    const required_major = try java_runtime.requiredMajor(input.metadata);
    if (!input.force_java) {
        if (input.runtime.major_version != required_major) return error.JavaVersionIncompatible;
        if (input.runtime.architecture == null or input.runtime.architecture.? != input.install.target.arch) {
            return error.JavaArchitectureIncompatible;
        }
    }

    const arena = try allocator.create(std.heap.ArenaAllocator);
    errdefer allocator.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(allocator);
    errdefer arena.deinit();
    const plan_allocator = arena.allocator();

    const assets_root = try std.fs.path.join(plan_allocator, &.{ input.root_directory, "assets" });
    const library_directory = try std.fs.path.join(plan_allocator, &.{ input.root_directory, "libraries" });
    const asset_index_name = if (input.metadata.assetIndex) |asset| asset.id else input.metadata.assets orelse "";
    const game_assets = if (hasVirtualAssets(input.install.asset_projections) and asset_index_name.len != 0)
        try std.fs.path.join(plan_allocator, &.{ input.root_directory, "assets", "virtual", asset_index_name })
    else
        assets_root;
    const classpath_separator = if (input.install.target.os == .windows) ";" else ":";
    const classpath = try buildClasspath(plan_allocator, input.root_directory, input.install.classpath, classpath_separator);
    const resolution_width = if (input.options.resolution_width) |width|
        try std.fmt.allocPrint(plan_allocator, "{d}", .{width})
    else
        "";
    const resolution_height = if (input.options.resolution_height) |height|
        try std.fmt.allocPrint(plan_allocator, "{d}", .{height})
    else
        "";
    const resolver: Resolver = .{
        .input = input,
        .allocator = plan_allocator,
        .assets_root = assets_root,
        .asset_index_name = asset_index_name,
        .game_assets = game_assets,
        .library_directory = library_directory,
        .classpath = classpath,
        .classpath_separator = classpath_separator,
        .resolution_width = resolution_width,
        .resolution_height = resolution_height,
    };

    var jvm_args: std.ArrayList([]const u8) = .empty;
    var game_args: std.ArrayList([]const u8) = .empty;
    var sensitive_jvm_indices: std.ArrayList(usize) = .empty;
    var sensitive_game_indices: std.ArrayList(usize) = .empty;

    if (input.metadata.arguments) |arguments| {
        if (input.options.include_default_user_jvm) {
            if (arguments.@"default-user-jvm") |defaults| {
                try appendArguments(plan_allocator, &jvm_args, &sensitive_jvm_indices, defaults, resolver);
            }
        }
        try appendArguments(plan_allocator, &jvm_args, &sensitive_jvm_indices, arguments.jvm, resolver);
        try appendArguments(plan_allocator, &game_args, &sensitive_game_indices, arguments.game, resolver);
    } else {
        const native_arg = try std.fmt.allocPrint(
            plan_allocator,
            "-Djava.library.path={s}",
            .{input.natives_directory},
        );
        try jvm_args.append(plan_allocator, native_arg);
        try jvm_args.append(plan_allocator, "-cp");
        try jvm_args.append(plan_allocator, classpath);

        const legacy = input.metadata.minecraftArguments orelse return error.MissingGameArguments;
        const tokens = try tokenizeLegacy(plan_allocator, legacy);
        for (tokens) |token| {
            const index = game_args.items.len;
            const expanded = try substitute(plan_allocator, token, resolver);
            try game_args.append(plan_allocator, expanded);
            if (isSensitiveTemplate(token)) try sensitive_game_indices.append(plan_allocator, index);
        }
    }

    for (input.options.extra_jvm_args) |argument| {
        const index = jvm_args.items.len;
        try jvm_args.append(plan_allocator, try plan_allocator.dupe(u8, argument));
        if (containsAuthSecret(argument, input.auth)) try sensitive_jvm_indices.append(plan_allocator, index);
    }
    if (input.metadata.logging) |logging| {
        const logging_path = try std.fs.path.join(plan_allocator, &.{
            input.root_directory,
            "assets",
            "log_configs",
            logging.client.file.id,
        });
        const logging_argument = try replaceSinglePlaceholder(
            plan_allocator,
            logging.client.argument,
            "path",
            logging_path,
        );
        try jvm_args.append(plan_allocator, logging_argument);
    }
    for (input.options.extra_game_args) |argument| {
        const index = game_args.items.len;
        try game_args.append(plan_allocator, try plan_allocator.dupe(u8, argument));
        if (containsAuthSecret(argument, input.auth)) try sensitive_game_indices.append(plan_allocator, index);
    }

    const owned_jvm_args = try jvm_args.toOwnedSlice(plan_allocator);
    const owned_game_args = try game_args.toOwnedSlice(plan_allocator);
    const argv_len = 1 + owned_jvm_args.len + 1 + owned_game_args.len;
    const argv = try plan_allocator.alloc([]const u8, argv_len);
    var argv_index: usize = 0;
    argv[argv_index] = try plan_allocator.dupe(u8, input.runtime.executable);
    argv_index += 1;
    for (owned_jvm_args) |argument| {
        argv[argv_index] = argument;
        argv_index += 1;
    }
    argv[argv_index] = try plan_allocator.dupe(u8, input.metadata.mainClass);
    argv_index += 1;
    for (owned_game_args) |argument| {
        argv[argv_index] = argument;
        argv_index += 1;
    }

    const sensitive_argv_indices = try plan_allocator.alloc(
        usize,
        sensitive_jvm_indices.items.len + sensitive_game_indices.items.len,
    );
    for (sensitive_jvm_indices.items, 0..) |index, output_index| {
        sensitive_argv_indices[output_index] = 1 + index;
    }
    const game_offset = 1 + owned_jvm_args.len + 1;
    for (sensitive_game_indices.items, 0..) |index, output_index| {
        sensitive_argv_indices[sensitive_jvm_indices.items.len + output_index] = game_offset + index;
    }
    const redacted_argv = try plan_allocator.dupe([]const u8, argv);
    for (sensitive_argv_indices) |index| redacted_argv[index] = "<redacted>";

    return .{
        .arena = arena,
        .value = .{
            .java_executable = argv[0],
            .working_directory = try plan_allocator.dupe(u8, input.game_directory),
            .jvm_args = owned_jvm_args,
            .main_class = argv[1 + owned_jvm_args.len],
            .game_args = owned_game_args,
            .argv = argv,
            .redacted_argv = redacted_argv,
            .sensitive_argv_indices = sensitive_argv_indices,
        },
    };
}

fn appendArguments(
    allocator: std.mem.Allocator,
    output: *std.ArrayList([]const u8),
    sensitive_indices: ?*std.ArrayList(usize),
    arguments: []const metadata.Argument,
    resolver: Resolver,
) !void {
    for (arguments) |argument| switch (argument) {
        .literal => |value| try appendExpanded(allocator, output, sensitive_indices, value, resolver),
        .structured => |structured| {
            if (!try rulesAllow(structured.rules, resolver.input)) continue;
            switch (structured.value) {
                .single => |value| try appendExpanded(allocator, output, sensitive_indices, value, resolver),
                .multiple => |values| for (values) |value| {
                    try appendExpanded(allocator, output, sensitive_indices, value, resolver);
                },
            }
        },
    };
}

fn appendExpanded(
    allocator: std.mem.Allocator,
    output: *std.ArrayList([]const u8),
    sensitive_indices: ?*std.ArrayList(usize),
    template: []const u8,
    resolver: Resolver,
) !void {
    const index = output.items.len;
    try output.append(allocator, try substitute(allocator, template, resolver));
    if (sensitive_indices) |indices| {
        if (isSensitiveTemplate(template)) try indices.append(allocator, index);
    }
}

fn substitute(allocator: std.mem.Allocator, template: []const u8, resolver: Resolver) ![]const u8 {
    var output: std.Io.Writer.Allocating = .init(allocator);
    errdefer output.deinit();
    var index: usize = 0;
    while (index < template.len) {
        const start = std.mem.indexOfPos(u8, template, index, "${") orelse {
            try output.writer.writeAll(template[index..]);
            break;
        };
        try output.writer.writeAll(template[index..start]);
        const end = std.mem.indexOfScalarPos(u8, template, start + 2, '}') orelse
            return error.InvalidPlaceholder;
        const name = template[start + 2 .. end];
        if (name.len == 0) return error.InvalidPlaceholder;
        const value = resolver.value(name) orelse return error.UnknownPlaceholder;
        try output.writer.writeAll(value);
        index = end + 1;
    }
    return output.toOwnedSlice();
}

fn replaceSinglePlaceholder(
    allocator: std.mem.Allocator,
    template: []const u8,
    name: []const u8,
    value: []const u8,
) ![]const u8 {
    const placeholder = try std.fmt.allocPrint(allocator, "${{{s}}}", .{name});
    return std.mem.replaceOwned(u8, allocator, template, placeholder, value);
}

fn tokenizeLegacy(allocator: std.mem.Allocator, input: []const u8) ![]const []const u8 {
    var tokens: std.ArrayList([]const u8) = .empty;
    var token: std.Io.Writer.Allocating = .init(allocator);
    defer token.deinit();
    var state: enum { normal, single_quote, double_quote } = .normal;
    var started = false;
    var index: usize = 0;
    while (index < input.len) : (index += 1) {
        const character = input[index];
        switch (state) {
            .normal => switch (character) {
                ' ', '\t', '\r', '\n' => {
                    if (started) {
                        try tokens.append(allocator, try token.toOwnedSlice());
                        token = .init(allocator);
                        started = false;
                    }
                },
                '\'' => {
                    state = .single_quote;
                    started = true;
                },
                '"' => {
                    state = .double_quote;
                    started = true;
                },
                '\\' => {
                    index += 1;
                    if (index == input.len) return error.InvalidLegacyArguments;
                    try token.writer.writeByte(input[index]);
                    started = true;
                },
                else => {
                    try token.writer.writeByte(character);
                    started = true;
                },
            },
            .single_quote => if (character == '\'') {
                state = .normal;
            } else {
                try token.writer.writeByte(character);
            },
            .double_quote => if (character == '"') {
                state = .normal;
            } else if (character == '\\') {
                index += 1;
                if (index == input.len) return error.InvalidLegacyArguments;
                try token.writer.writeByte(input[index]);
            } else {
                try token.writer.writeByte(character);
            },
        }
    }
    if (state != .normal) return error.InvalidLegacyArguments;
    if (started) try tokens.append(allocator, try token.toOwnedSlice());
    return tokens.toOwnedSlice(allocator);
}

fn rulesAllow(rules: []const metadata.Rule, input: Input) !bool {
    if (rules.len == 0) return true;
    var allowed = false;
    for (rules) |rule| {
        if (try ruleMatches(rule, input)) allowed = rule.action == .allow;
    }
    return allowed;
}

fn ruleMatches(rule: metadata.Rule, input: Input) !bool {
    if (rule.os) |os| {
        if (os.name) |name| {
            if (!std.mem.eql(u8, name, @tagName(input.install.target.os))) return false;
        }
        if (os.arch) |pattern| {
            if (!try regexMatches(pattern, input.runtime.os_arch)) return false;
        }
        if (os.version) |pattern| {
            const version = input.runtime.os_version orelse return false;
            if (!try regexMatches(pattern, version)) return false;
        }
        if (os.versionRange) |range| {
            const version = input.runtime.os_version orelse return false;
            if (range.min) |minimum| {
                if (try compareNumericVersions(version, minimum) == .lt) return false;
            }
            if (range.max) |maximum| {
                if (try compareNumericVersions(version, maximum) == .gt) return false;
            }
        }
    }
    if (rule.features) |features| {
        var iterator = features.map.iterator();
        while (iterator.next()) |entry| {
            if (featureValue(entry.key_ptr.*, input.options) != entry.value_ptr.*) return false;
        }
    }
    return true;
}

fn featureValue(name: []const u8, options: Options) bool {
    if (std.mem.eql(u8, name, "is_demo_user")) return options.demo;
    if (std.mem.eql(u8, name, "has_custom_resolution")) {
        return options.resolution_width != null and options.resolution_height != null;
    }
    if (std.mem.eql(u8, name, "has_quick_plays_support")) return options.quick_play_support;
    if (std.mem.eql(u8, name, "is_quick_play_singleplayer")) return options.quick_play_singleplayer.len != 0;
    if (std.mem.eql(u8, name, "is_quick_play_multiplayer")) return options.quick_play_multiplayer.len != 0;
    if (std.mem.eql(u8, name, "is_quick_play_realms")) return options.quick_play_realms.len != 0;
    return false;
}

fn regexMatches(pattern: []const u8, text: []const u8) !bool {
    if (pattern.len > 256 or text.len > 1024) return error.RulePatternTooLong;
    const anchored = pattern.len != 0 and pattern[0] == '^';
    const start_pattern: usize = if (anchored) 1 else 0;
    if (anchored) return regexMatchHere(pattern, start_pattern, text, 0);
    for (0..text.len + 1) |start| {
        if (try regexMatchHere(pattern, 0, text, start)) return true;
    }
    return false;
}

fn regexMatchHere(pattern: []const u8, pattern_index: usize, text: []const u8, text_index: usize) !bool {
    if (pattern_index == pattern.len) return true;
    if (pattern[pattern_index] == '$' and pattern_index + 1 == pattern.len) return text_index == text.len;

    const atom = try parseRegexAtom(pattern, pattern_index);
    const has_star = atom.next < pattern.len and pattern[atom.next] == '*';
    if (has_star) {
        var end = text_index;
        while (end < text.len and atomMatches(atom, text[end])) : (end += 1) {}
        while (true) {
            if (try regexMatchHere(pattern, atom.next + 1, text, end)) return true;
            if (end == text_index) break;
            end -= 1;
        }
        return false;
    }
    if (text_index == text.len or !atomMatches(atom, text[text_index])) return false;
    return regexMatchHere(pattern, atom.next, text, text_index + 1);
}

const RegexAtom = struct {
    kind: enum { literal, any, digit },
    literal: u8 = 0,
    next: usize,
};

fn parseRegexAtom(pattern: []const u8, index: usize) !RegexAtom {
    const character = pattern[index];
    if (character == '.') return .{ .kind = .any, .next = index + 1 };
    if (character == '\\') {
        if (index + 1 == pattern.len) return error.UnsupportedRulePattern;
        if (pattern[index + 1] == 'd') return .{ .kind = .digit, .next = index + 2 };
        return .{ .kind = .literal, .literal = pattern[index + 1], .next = index + 2 };
    }
    if (std.mem.indexOfScalar(u8, "[](){}+?|^", character) != null) {
        return error.UnsupportedRulePattern;
    }
    return .{ .kind = .literal, .literal = character, .next = index + 1 };
}

fn atomMatches(atom: RegexAtom, character: u8) bool {
    return switch (atom.kind) {
        .literal => character == atom.literal,
        .any => true,
        .digit => std.ascii.isDigit(character),
    };
}

fn compareNumericVersions(left: []const u8, right: []const u8) !std.math.Order {
    var left_parts = std.mem.splitScalar(u8, left, '.');
    var right_parts = std.mem.splitScalar(u8, right, '.');
    while (true) {
        const left_part = left_parts.next();
        const right_part = right_parts.next();
        if (left_part == null and right_part == null) return .eq;
        const left_value = if (left_part) |part|
            std.fmt.parseInt(u64, part, 10) catch return error.UnsupportedVersionRange
        else
            0;
        const right_value = if (right_part) |part|
            std.fmt.parseInt(u64, part, 10) catch return error.UnsupportedVersionRange
        else
            0;
        if (left_value < right_value) return .lt;
        if (left_value > right_value) return .gt;
    }
}

fn buildClasspath(
    allocator: std.mem.Allocator,
    root: []const u8,
    relative_paths: []const []const u8,
    separator: []const u8,
) ![]const u8 {
    var paths = try allocator.alloc([]const u8, relative_paths.len);
    for (relative_paths, 0..) |path, index| {
        paths[index] = try std.fs.path.join(allocator, &.{ root, path });
    }
    return std.mem.join(allocator, separator, paths);
}

fn hasVirtualAssets(projections: []const install_plan.AssetProjection) bool {
    for (projections) |projection| {
        if (projection.kind == .virtual_assets) return true;
    }
    return false;
}

fn isSensitiveTemplate(template: []const u8) bool {
    return std.mem.indexOf(u8, template, "${auth_access_token}") != null or
        std.mem.indexOf(u8, template, "${auth_session}") != null;
}

fn containsAuthSecret(argument: []const u8, auth: Auth) bool {
    if (auth.access_token.len != 0 and std.mem.indexOf(u8, argument, auth.access_token) != null) return true;
    if (auth.session) |session| {
        if (session.len != 0 and std.mem.indexOf(u8, argument, session) != null) return true;
    }
    return false;
}

test "builds modern launch arguments with rule features and redaction" {
    const source =
        \\{
        \\  "id":"test-modern",
        \\  "type":"release",
        \\  "mainClass":"example.Main",
        \\  "arguments":{
        \\    "game":["--username","${auth_player_name}","--token","${auth_access_token}",{"rules":[{"action":"allow","features":{"has_custom_resolution":true}}],"value":["--width","${resolution_width}"]}],
        \\    "jvm":["-Djava.library.path=${natives_directory}","-cp","${classpath}","-Dtoken=${auth_access_token}"]
        \\  },
        \\  "downloads":{"client":{"sha1":"1111111111111111111111111111111111111111","size":1,"url":"https://example.invalid/client.jar"}},
        \\  "libraries":[],
        \\  "javaVersion":{"component":"test","majorVersion":17}
        \\}
    ;
    var parsed = try metadata.parse(std.testing.allocator, source);
    defer parsed.deinit();
    const classpath = [_][]const u8{ "libraries/test.jar", "versions/test-modern/test-modern.jar" };
    const install: install_plan.Data = .{
        .version_id = "test-modern",
        .source = .official,
        .target = .{ .os = .linux, .arch = .x86_64 },
        .files = &.{},
        .classpath = &classpath,
        .native_extractions = &.{},
        .asset_projections = &.{},
        .total_download_size = 0,
    };
    const runtime: java_runtime.Runtime = .{
        .executable = "/java/bin/java",
        .source = .explicit,
        .major_version = 17,
        .specification_version = "17",
        .version = "17.0.1",
        .runtime_version = null,
        .vendor = null,
        .java_home = null,
        .vm_name = null,
        .os_arch = "amd64",
        .os_version = "6.8.0",
        .architecture = .x86_64,
        .data_model = 64,
    };
    var plan = try build(std.testing.allocator, .{
        .metadata = parsed.value,
        .install = install,
        .runtime = runtime,
        .root_directory = "/game",
        .game_directory = "/instance",
        .natives_directory = "/natives",
        .auth = .{
            .player_name = "Player",
            .uuid = "uuid",
            .access_token = "secret-token",
        },
        .options = .{
            .resolution_width = 1280,
            .resolution_height = 720,
            .extra_jvm_args = &.{"-Dliteral=secret-token"},
            .extra_game_args = &.{"--literal=secret-token"},
        },
    });
    defer plan.deinit();

    try std.testing.expectEqualStrings("/java/bin/java", plan.value.argv[0]);
    const expected_library = try std.fs.path.join(std.testing.allocator, &.{ "/game", "libraries/test.jar" });
    defer std.testing.allocator.free(expected_library);
    try std.testing.expect(std.mem.indexOf(u8, plan.value.jvm_args[2], expected_library) != null);
    try std.testing.expectEqualStrings("Player", plan.value.game_args[1]);
    try std.testing.expectEqualStrings("1280", plan.value.game_args[5]);
    try std.testing.expectEqual(@as(usize, 4), plan.value.sensitive_argv_indices.len);
    try std.testing.expectEqualStrings("-Dtoken=secret-token", plan.value.argv[plan.value.sensitive_argv_indices[0]]);
    for (plan.value.sensitive_argv_indices) |index| {
        try std.testing.expectEqualStrings("<redacted>", plan.value.redacted_argv[index]);
    }
}

test "tokenizes legacy quoted arguments before substitution" {
    const tokens = try tokenizeLegacy(
        std.testing.allocator,
        "--gameDir \"${game_directory}\" --empty ''",
    );
    defer {
        for (tokens) |token| std.testing.allocator.free(token);
        std.testing.allocator.free(tokens);
    }
    try std.testing.expectEqual(@as(usize, 4), tokens.len);
    try std.testing.expectEqualStrings("${game_directory}", tokens[1]);
    try std.testing.expectEqualStrings("", tokens[3]);
}

test "builds legacy launch arguments without shell splitting" {
    const source =
        \\{
        \\  "id":"legacy",
        \\  "type":"old_alpha",
        \\  "mainClass":"legacy.Main",
        \\  "minecraftArguments":"--username ${auth_player_name} --session ${auth_session} --gameDir \"${game_directory}\"",
        \\  "downloads":{"client":{"sha1":"1111111111111111111111111111111111111111","size":1,"url":"https://example.invalid/client.jar"}},
        \\  "libraries":[]
        \\}
    ;
    var parsed = try metadata.parse(std.testing.allocator, source);
    defer parsed.deinit();
    const classpath = [_][]const u8{"versions/legacy/legacy.jar"};
    const install: install_plan.Data = .{
        .version_id = "legacy",
        .source = .official,
        .target = .{ .os = .windows, .arch = .x86_64 },
        .files = &.{},
        .classpath = &classpath,
        .native_extractions = &.{},
        .asset_projections = &.{},
        .total_download_size = 0,
    };
    const runtime: java_runtime.Runtime = .{
        .executable = "C:/Java/bin/java.exe",
        .source = .explicit,
        .major_version = 8,
        .specification_version = "1.8",
        .version = "1.8.0_402",
        .runtime_version = null,
        .vendor = null,
        .java_home = null,
        .vm_name = null,
        .os_arch = "amd64",
        .os_version = "10.0.22631",
        .architecture = .x86_64,
        .data_model = 64,
    };
    var plan = try build(std.testing.allocator, .{
        .metadata = parsed.value,
        .install = install,
        .runtime = runtime,
        .root_directory = "C:/Game Data",
        .game_directory = "C:/Instances/My World",
        .natives_directory = "C:/Game Data/natives",
        .auth = .{
            .player_name = "Player",
            .uuid = "uuid",
            .access_token = "token",
            .session = "token:token:uuid",
        },
    });
    defer plan.deinit();

    try std.testing.expectEqualStrings("-Djava.library.path=C:/Game Data/natives", plan.value.jvm_args[0]);
    try std.testing.expectEqualStrings("C:/Instances/My World", plan.value.game_args[5]);
    try std.testing.expectEqualStrings("token:token:uuid", plan.value.game_args[3]);
    try std.testing.expectEqual(@as(usize, 1), plan.value.sensitive_argv_indices.len);
}

test "matches Mojang OS rule patterns" {
    try std.testing.expect(try regexMatches("^10\\.5\\.\\d$", "10.5.8"));
    try std.testing.expect(!try regexMatches("^10\\.5\\.\\d$", "10.6.8"));
    try std.testing.expectEqual(std.math.Order.gt, try compareNumericVersions("10.0.22000", "10.0.17134"));
}
