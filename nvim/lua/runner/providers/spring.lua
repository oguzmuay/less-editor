local M = {}

--- Debug modunda JVM, debugger bağlanana kadar bekletilsin mi (JDWP `suspend=y`). Varsayılan `false`: uygulama
--- jdtls hazır olmasa da normal açılır; JDWP satırı JVM açılışında (Spring context'ten önce) basıldığı için
--- jdtls zaten çalışıyorsa debugger 1-2 sn içinde bağlanır. Çok erken açılış kodunu ayıklamak için `true` yapılabilir.
M.suspend = false

--- jdtls'in debug için hazır olmasını (başlatma + Maven/Gradle import + java-debug bundle'ı) bekleme süresi (ms).
M.jdtls_timeout = 180000

--- İlk JDWP portu; doluysa sıradaki boş port seçilir.
local JDWP_BASE_PORT = 5005

local GRADLE_INIT = vim.fs.joinpath(vim.fs.dirname(debug.getinfo(1, "S").source:sub(2)), "jdwp.gradle")

--- JDWP ajanının seçenekleri. Sadece 127.0.0.1'e bağlanır: JDWP kimlik doğrulamasızdır, ağa açmak uzaktan kod
--- çalıştırmaya izin verir.
---@param item RunnerItem
---@return string
local function jdwp(item)
  return ("transport=dt_socket,server=y,suspend=%s,address=127.0.0.1:%d"):format(
    M.suspend and "y" or "n",
    item.debug_port
  )
end

--- Spring Boot uygulamasını çalıştıracak komutu oluşturur. Proje içinde mvnw/gradlew varsa onu, yoksa global aracı kullanır.
--- Debug modunda (`item.mode == "debug"`) JDWP ajanı eklenir: Maven'da `spring-boot.run.jvmArguments`, Gradle'da
--- `bootRun` görevine jvmArgs ekleyen init script'i (`jdwp.gradle`, `-PrunnerJdwp=`).
---@param item RunnerItem
---@return string[]
local function build_cmd(item)
  local debugging = item.mode == "debug" and item.debug_port
  if item.tool == "maven" then
    local wrapper = vim.fs.find("mvnw", { upward = true, path = item.cwd, type = "file" })[1]
    local cmd = { wrapper or "mvn", "spring-boot:run" }
    if item.profile then
      table.insert(cmd, "-Dspring-boot.run.profiles=" .. item.profile)
    end
    if debugging then
      table.insert(cmd, "-Dspring-boot.run.jvmArguments=-agentlib:jdwp=" .. jdwp(item))
    end
    return cmd
  end
  local wrapper = vim.fs.find("gradlew", { upward = true, path = item.cwd, type = "file" })[1]
  local cmd = { wrapper or "gradle", "bootRun" }
  if item.profile then
    table.insert(cmd, "--args=--spring.profiles.active=" .. item.profile)
  end
  if debugging then
    vim.list_extend(cmd, { "--init-script", GRADLE_INIT, "-PrunnerJdwp=" .. jdwp(item) })
  end
  return cmd
end

--- Kaynak dosyadaki `package` satırından sınıfın tam adını çıkarır.
---@param file string
---@return string
local function main_class(file)
  local name = vim.fn.fnamemodify(file, ":t:r")
  local ok, lines = pcall(vim.fn.readfile, file, "", 200)
  for _, line in ipairs(ok and lines or {}) do
    local pkg = line:match("^%s*package%s+([%w_%.]+)%s*;")
    if pkg then
      return pkg .. "." .. name
    end
  end
  return name
end

--- jdtls istemcisi java-debug eklentisiyle debug oturumu açabiliyor mu (LazyVim `setup_dap`'i LspAttach'te çağırır).
---@param client vim.lsp.Client
---@return boolean
local function can_debug(client)
  local commands = vim.tbl_get(client.server_capabilities, "executeCommandProvider", "commands") or {}
  return require("dap").adapters.java ~= nil and vim.tbl_contains(commands, "vscode.java.startDebugSession")
end

--- java-debug-adapter Mason paketi kurulu mu (kurulu değilse jdtls'e debug bundle'ı verilmez).
---@return boolean
local function has_debug_adapter()
  local ok, registry = pcall(require, "mason-registry")
  return not ok or registry.is_installed("java-debug-adapter")
end

--- JDWP portu açılmış JVM'e nvim-dap `java` attach ile bağlanır. Adapter jdtls'in java-debug bundle'ıdır; bu yüzden
--- ana sınıfın dosyası bir buffer'a yüklenir (jdtls kapalıysa FileType ile başlar) ve jdtls hazır olana kadar
--- beklenir. `projectName` jdtls'ten çözülür (ifade değerlendirme ve kaynak eşleme için); `modulePaths`/`classPaths`/
--- `javaExec` sadece nvim-jdtls'in launch'a yönelik zenginleştirmesini atlatmak için doldurulur.
---@param item RunnerItem
---@param port integer
---@param run fun(config: table)
local function attach(item, port, run)
  if not has_debug_adapter() then
    vim.notify(
      "java-debug-adapter is not installed (:MasonInstall java-debug-adapter). JDWP is listening on 127.0.0.1:" .. port,
      vim.log.levels.WARN,
      { title = "Run" }
    )
    return
  end
  local buf = vim.fn.bufadd(item.file)
  if not vim.api.nvim_buf_is_loaded(buf) then
    vim.fn.bufload(buf)
  end
  item.dap_state = "waiting"
  require("runner").refresh()
  local client
  require("runner.dap").wait(function()
    if not item.job or item.debug_port ~= port then
      item.dap_state = nil
      require("runner").refresh()
      return false
    end
    client = vim.lsp.get_clients({ bufnr = buf, name = "jdtls" })[1]
    return client and can_debug(client) or nil
  end, function()
    local cls = main_class(item.file)
    client:request("workspace/executeCommand", { command = "vscode.java.resolveMainClass" }, function(err, res)
      local project = ""
      for _, entry in ipairs(not err and type(res) == "table" and res or {}) do
        if entry.mainClass == cls then
          project = entry.projectName
          break
        end
      end
      run({
        type = "java",
        request = "attach",
        name = "Debug " .. item.label,
        hostName = "127.0.0.1",
        port = port,
        cwd = client.config.root_dir,
        mainClass = cls,
        projectName = project,
        modulePaths = {},
        classPaths = {},
        javaExec = "java",
      })
    end, buf)
  end, function()
    item.dap_state = nil
    require("runner").refresh()
    vim.notify(
      ("Debugger not attached: jdtls is not ready for %s. JDWP is listening on 127.0.0.1:%d; press d to retry."):format(
        item.label,
        port
      ),
      vim.log.levels.WARN,
      { title = "Run" }
    )
  end, M.jdtls_timeout)
end

--- Kökteki @SpringBootApplication sınıflarını bulup "Spring Boot" grubunu oluşturur.
---@param root string
---@return RunnerNode?
function M.discover(root)
  local res = vim
    .system({
      "rg",
      "--files-with-matches",
      "--glob=*.java",
      "--glob=*.kt",
      "--glob=!**/target/**",
      "--glob=!**/build/**",
      "--glob=!**/node_modules/**",
      "-e",
      "@SpringBootApplication",
      root,
    }, { text = true })
    :wait()
  local items = {}
  for file in vim.gsplit(res.stdout or "", "\n", { trimempty = true }) do
    local module_dir = vim.fs.root(file, { "pom.xml", "build.gradle", "build.gradle.kts" })
    if module_dir then
      local tool = vim.uv.fs_stat(module_dir .. "/pom.xml") and "maven" or "gradle"
      local tool_icon = require("mini.icons").get("file", tool == "maven" and "pom.xml" or "build.gradle")
      table.insert(items, {
        id = "spring:" .. file,
        label = vim.fn.fnamemodify(file, ":t:r"),
        detail = tool_icon .. " " .. vim.fs.basename(module_dir),
        icon = "\u{F06C}",
        icon_hl = "RunnerSpring",
        cwd = module_dir,
        file = file,
        tool = tool,
        cmd = build_cmd,
        ready = "Started %S+ in [%d%.]+ seconds",
        profiles = true,
        debug = {
          port = JDWP_BASE_PORT,
          ready = "Listening for transport dt_socket at address: (%d+)",
          attach = file:match("%.java$") and attach or nil,
        },
      })
    end
  end
  table.sort(items, function(a, b)
    return a.label < b.label
  end)
  return { id = "spring", label = "Spring Boot", icon = "\u{F06C}", icon_hl = "RunnerSpring", children = items }
end

return M
