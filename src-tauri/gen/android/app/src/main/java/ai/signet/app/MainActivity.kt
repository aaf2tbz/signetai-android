package ai.signet.app

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.util.Log
import androidx.activity.enableEdgeToEdge
import java.io.File
import java.io.FileOutputStream
import java.net.HttpURLConnection
import java.net.URL

class MainActivity : TauriActivity() {
    companion object {
        private const val TAG = "Signet"
        private const val DAEMON_ASSET = "signet-daemon"
        private const val LLAMA_SERVER_ASSET = "llama-server"
        private const val AGENTS_DIR_NAME = ".agents"
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        extractBinaryIfNeeded(DAEMON_ASSET, "signet-daemon")
        extractBinaryIfNeeded(LLAMA_SERVER_ASSET, "llama-server")
        extractModelIfNeeded()
        ensureAgentConfig()
        startForegroundService()
        super.onCreate(savedInstanceState)
        handleIntent(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleIntent(intent)
    }

    private fun extractBinaryIfNeeded(assetName: String, binaryName: String) {
        try {
            val agentsDir = File(filesDir, AGENTS_DIR_NAME)
            val binDir = File(agentsDir, "bin")
            binDir.mkdirs()

            val outFile = File(binDir, binaryName)

            if (outFile.exists() && outFile.canExecute()) {
                Log.d(TAG, "$binaryName already extracted (${outFile.length()} bytes)")
                return
            }

            Log.i(TAG, "Extracting $binaryName from APK assets...")
            assets.open(assetName).use { input ->
                FileOutputStream(outFile).use { output ->
                    val buf = ByteArray(8192)
                    var len: Int
                    while (input.read(buf).also { len = it } != -1) {
                        output.write(buf, 0, len)
                    }
                }
            }

            outFile.setExecutable(true, false)
            outFile.setReadable(true, false)
            outFile.setWritable(true, true)

            Log.i(TAG, "$binaryName extracted: ${outFile.absolutePath} (${outFile.length()} bytes)")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to extract $binaryName", e)
        }
    }

    private fun extractModelIfNeeded() {
        val agentsDir = File(filesDir, AGENTS_DIR_NAME)
        val modelsDir = File(agentsDir, "models")
        modelsDir.mkdirs()

        val models = listOf(
            "nomic-embed-text-v1.5.Q4_K_M.gguf" to "nomic-embed-text-v1.5.Q4_K_M.gguf",
            "Qwen2.5-1.5B-Instruct-Q4_0_4_8.gguf" to "Qwen2.5-1.5B-Instruct-Q4_0_4_8.gguf"
        )

        for ((asset, filename) in models) {
            try {
                val modelFile = File(modelsDir, filename)
                if (modelFile.exists()) {
                    Log.d(TAG, "Model $filename already extracted")
                    continue
                }

                Log.i(TAG, "Extracting $filename from APK assets...")
                assets.open(asset).use { input ->
                    FileOutputStream(modelFile).use { output ->
                        val buf = ByteArray(8192)
                        var len: Int
                        while (input.read(buf).also { len = it } != -1) {
                            output.write(buf, 0, len)
                        }
                    }
                }
                Log.i(TAG, "$filename extracted (${modelFile.length()} bytes)")
            } catch (e: Exception) {
                Log.e(TAG, "Failed to extract $filename (optional)", e)
            }
        }
    }

    private fun ensureAgentConfig() {
        try {
            val agentsDir = File(filesDir, AGENTS_DIR_NAME)
            agentsDir.mkdirs()
            val daemonDir = File(agentsDir, ".daemon/logs")
            daemonDir.mkdirs()
            val memDir = File(agentsDir, "memory")
            memDir.mkdirs()

            val agentYaml = File(agentsDir, "agent.yaml")
            if (agentYaml.exists()) {
                val content = agentYaml.readText()
                if (!content.contains("configVersion:")) {
                    Log.i(TAG, "Replacing legacy agent.yaml with full config")
                    agentYaml.delete()
                }
            }
            if (!agentYaml.exists()) {
                Log.i(TAG, "Creating default agent.yaml")
                val now = java.time.Instant.now().toString()
                val yaml = """
                    |configVersion: 2
                    |version: 1
                    |schema: signet/v1
                    |agent:
                    |  name: Signet
                    |  description: "Signet mobile agent"
                    |  created: $now
                    |  updated: $now
                    |network:
                    |  mode: localhost
                    |harnesses: []
                    |memory:
                    |  database: memory/memories.db
                    |  session_budget: 2000
                    |  decay_rate: 0.95
                    |  pipelineV2:
                    |    enabled: true
                    |    extraction:
                    |      provider: llama-cpp
                    |      model: qwen2.5:1.5b
                    |    synthesis:
                    |      enabled: true
                    |      provider: llama-cpp
                    |      model: qwen2.5:1.5b
                    |      timeout: 300000
                    |search:
                    |  alpha: 0.7
                    |  top_k: 20
                    |  min_score: 0.3
                    |embedding:
                    |  provider: llama-cpp
                    |  model: nomic-embed-text
                    |  dimensions: 768
                """.trimMargin()
                agentYaml.writeText(yaml)
            }

            val agentsMd = File(agentsDir, "AGENTS.md")
            if (!agentsMd.exists()) {
                agentsMd.writeText("# Agent Instructions\n\nSignet mobile agent.\n")
            }

            val soulMd = File(agentsDir, "SOUL.md")
            if (!soulMd.exists()) {
                soulMd.writeText("# SOUL.md\n\nSignet mobile companion.\n")
            }

            val identityMd = File(agentsDir, "IDENTITY.md")
            if (!identityMd.exists()) {
                identityMd.writeText("# IDENTITY.md\n\n- **Name:** Signet\n- **Creature:** Mobile companion\n")
            }

            val userMd = File(agentsDir, "USER.md")
            if (!userMd.exists()) {
                userMd.writeText("# USER.md\n\n- **Preferred name:** User\n- **Language:** English\n")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to create agent config", e)
        }
    }

    private fun startForegroundService() {
        try {
            val intent = Intent(this, SignetDaemonService::class.java)
            startForegroundService(intent)
            Log.d(TAG, "Foreground service started")
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start foreground service", e)
        }
    }

    private fun handleIntent(intent: Intent?) {
        if (intent == null) return

        if (intent.action == Intent.ACTION_SEND && intent.type == "text/plain") {
            val sharedText = intent.getStringExtra(Intent.EXTRA_TEXT)
            if (!sharedText.isNullOrEmpty()) {
                Log.d(TAG, "Received shared text (${sharedText.length} chars)")
                ingestText(sharedText)
            }
        }
    }

    private fun ingestText(text: String) {
        Thread {
            try {
                val url = URL("http://localhost:3850/api/memory/remember")
                val conn = url.openConnection() as HttpURLConnection
                conn.requestMethod = "POST"
                conn.setRequestProperty("Content-Type", "application/json")
                conn.doOutput = true
                conn.connectTimeout = 5000
                conn.readTimeout = 5000

                val json = """{"content":${escapeJson(text)},"who":"android-share","importance":0.7}"""
                conn.outputStream.use { it.write(json.toByteArray()) }

                val code = conn.responseCode
                Log.d(TAG, "Ingest response: $code")
                conn.disconnect()
            } catch (e: Exception) {
                Log.e(TAG, "Failed to ingest shared text", e)
            }
        }.start()
    }

    private fun escapeJson(s: String): String {
        val sb = StringBuilder("\"")
        for (c in s) {
            when (c) {
                '"' -> sb.append("\\\"")
                '\\' -> sb.append("\\\\")
                '\n' -> sb.append("\\n")
                '\r' -> sb.append("\\r")
                '\t' -> sb.append("\\t")
                else -> sb.append(c)
            }
        }
        sb.append("\"")
        return sb.toString()
    }
}
