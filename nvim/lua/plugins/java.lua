local build_files = { "pom.xml", "build.gradle", "build.gradle.kts" }

--- Mason'daki güncel jdtls'in (1.63+) çalışmak için istediği en düşük Java sürümü.
local JDTLS_MIN_JAVA = 25

--- Projeler için varsayılan JDK (pom'da sürüm belirtilmemişse jdtls bunu kullanır).
local DEFAULT_PROJECT_JAVA = 21

--- Monorepo içinde bir Java dosyasının ait olduğu projenin kökünü bulur.
--- Sıra: Gradle settings dosyası → iç içe pom.xml/build.gradle zincirinin en üstü → wrapper/.git.
--- Böylece backend'de mvnw olmasa bile kök, monorepo kökü (FE dahil) yerine backend klasörü olur.
---@param path string
---@return string?
local function java_root(path)
  local settings = vim.fs.root(path, { "settings.gradle", "settings.gradle.kts" })
  if settings then
    return settings
  end
  local function has_build_file(dir)
    return vim.iter(build_files):any(function(f)
      return vim.uv.fs_stat(dir .. "/" .. f) ~= nil
    end)
  end
  local dir = vim.fs.root(path, build_files)
  while dir do
    local parent = vim.fs.dirname(dir)
    if parent == dir or not has_build_file(parent) then
      return dir
    end
    dir = parent
  end
  return vim.fs.root(path, { "mvnw", "gradlew", ".git" })
end

--- Makinedeki JDK'ları (Homebrew openjdk*, /Library/Java) bulur; her ana sürümden bir tane döner.
--- Sürüm, JDK kökündeki `release` dosyasının JAVA_VERSION satırından okunur.
---@return table<integer, string> ana sürüm -> JAVA_HOME
local function find_jdks()
  local homes = vim.fn.glob("/opt/homebrew/opt/openjdk*/libexec/openjdk.jdk/Contents/Home", false, true)
  vim.list_extend(homes, vim.fn.glob("/Library/Java/JavaVirtualMachines/*/Contents/Home", false, true))
  local jdks = {}
  for _, home in ipairs(homes) do
    local ok, lines = pcall(vim.fn.readfile, home .. "/release")
    local major = ok and tonumber(table.concat(lines, "\n"):match('JAVA_VERSION="(%d+)'))
    if major and not jdks[major] and vim.uv.fs_stat(home .. "/bin/java") then
      jdks[major] = home
    end
  end
  return jdks
end

--- jdtls sunucusunu çalıştıracak en yeni JDK'nın java yolunu döner (en az JDTLS_MIN_JAVA).
--- Bulunamazsa kullanıcıyı uyarır ve nil döner (jdtls betiği PATH'teki java'yı dener).
---@param jdks table<integer, string>
---@return string?
local function jdtls_java(jdks)
  local best = vim.iter(vim.tbl_keys(jdks)):fold(nil, function(acc, major)
    return (major >= JDTLS_MIN_JAVA and (not acc or major > acc)) and major or acc
  end)
  if best then
    return jdks[best] .. "/bin/java"
  end
  vim.schedule(function()
    vim.notify(
      ("jdtls requires Java %d+ but none was found. Install: brew install openjdk"):format(JDTLS_MIN_JAVA),
      vim.log.levels.ERROR,
      { title = "jdtls" }
    )
  end)
end

--- Bulunan JDK'ları jdtls'in `java.configuration.runtimes` ayarına çevirir.
--- Projeler pom/gradle'daki sürüme uyan JDK ile derlenir; eşleşme yoksa DEFAULT_PROJECT_JAVA kullanılır.
---@param jdks table<integer, string>
---@return table[]
local function java_runtimes(jdks)
  local majors = vim.tbl_keys(jdks)
  table.sort(majors)
  return vim.tbl_map(function(major)
    return { name = "JavaSE-" .. major, path = jdks[major], default = major == DEFAULT_PROJECT_JAVA or nil }
  end, majors)
end

--- LazyVim'in `java-test/*.jar` ile topladığı bundle listesinden OSGi bundle'ı olmayan jar'ları
--- (test runner, jacoco agent) ve jdtls'in zaten içerdiği jar'ları (ör. org.objectweb.asm) çıkarır.
--- Bunlar kalırsa jdtls açılışta "Failed to load extension bundles" hatası yazar.
---@param config table jdtls başlatma yapılandırması
---@return table
local function filter_bundles(config)
  local skip = { ["com.microsoft.java.test.runner-jar-with-dependencies.jar"] = true, ["jacocoagent.jar"] = true }
  local jdtls_plugins = vim.fn.expand("$MASON/share/jdtls/plugins/")
  local bundles = vim.tbl_get(config, "init_options", "bundles") or {}
  config.init_options.bundles = vim.tbl_filter(function(jar)
    local name = vim.fs.basename(jar)
    return not skip[name] and not vim.uv.fs_stat(jdtls_plugins .. name)
  end, bundles)
  return config
end

return {
  {
    "mfussenegger/nvim-jdtls",
    opts = function(_, opts)
      local jdks = find_jdks()
      local java = jdtls_java(jdks)
      if java then
        table.insert(opts.cmd, 2, "--java-executable=" .. java)
      end
      table.insert(opts.cmd, "--jvm-arg=-Djava.import.generatesMetadataFilesAtProjectRoot=false")
      vim.lsp.commands["_java.reloadBundles.command"] = function()
        return {}
      end
      opts.jdtls = filter_bundles
      opts.root_dir = java_root
      opts.project_name = function(root_dir)
        return root_dir and (vim.fs.basename(root_dir) .. "-" .. vim.fn.sha256(root_dir):sub(1, 8))
      end
      opts.settings = vim.tbl_deep_extend("force", opts.settings or {}, {
        java = { configuration = { runtimes = java_runtimes(jdks) } },
      })
    end,
  },
}
