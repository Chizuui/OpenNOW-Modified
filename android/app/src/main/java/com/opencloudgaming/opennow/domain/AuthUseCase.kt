package com.opencloudgaming.opennow.domain

import android.app.Application
import android.util.Log
import com.opencloudgaming.opennow.AuthSession
import com.opencloudgaming.opennow.AuthStore
import com.opencloudgaming.opennow.AuthRepository
import com.opencloudgaming.opennow.LoginProvider
import com.opencloudgaming.opennow.defaultProvider
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

private const val TAG = "AuthUseCase"

/**
 * Use case handling authentication operations.
 * Extracted from OpenNowViewModel for better separation of concerns.
 */
class AuthUseCase(
    private val authStore: AuthStore,
    private val authRepository: AuthRepository,
) {
    private val authRestoreMutex = Mutex()

    /**
     * Restore an existing auth session.
     */
    suspend fun restoreSession(throwOnRefreshFailure: Boolean = false): Result<AuthSession?> {
        return authRestoreMutex.withLock {
            runCatching {
                authRepository.restore(
                    throwOnRefreshFailure = throwOnRefreshFailure,
                    removeExpiredSessionOnFailure = !throwOnRefreshFailure,
                )
            }
        }
    }

    /**
     * Login with the best available method for the given provider.
     */
    suspend fun loginWithBestMethod(
        provider: LoginProvider,
        useDeviceCode: Boolean,
        onPhaseChanged: (String) -> Unit,
    ): AuthSession {
        if (useDeviceCode) {
            return loginWithDeviceCode(provider, onPhaseChanged)
        }

        return try {
            authRepository.login(provider) {
                onPhaseChanged("Getting sign-in tokens")
            }
        } catch (error: Throwable) {
            if (error is CancellationException || !isLoopbackLoginFailure(error)) {
                throw error
            }
            if (!provider.supportsDeviceCodeLogin) {
                throw error
            }
            onPhaseChanged("Requesting sign-in code")
            loginWithDeviceCode(provider, onPhaseChanged)
        }
    }

    /**
     * Login with device code flow.
     */
    suspend fun loginWithDeviceCode(
        provider: LoginProvider,
        onPhaseChanged: (String) -> Unit,
    ): AuthSession {
        return authRepository.loginWithDeviceCode(provider) { prompt ->
            onPhaseChanged("Waiting for sign-in")
        }
    }

    /**
     * Login with a token.
     */
    suspend fun loginWithToken(
        tokenInput: String,
        provider: LoginProvider,
    ): AuthSession {
        return authRepository.loginWithToken(provider, tokenInput)
    }

    /**
     * Logout and clean up.
     */
    suspend fun logout() {
        authRepository.logout()
    }

    /**
     * Switch to a different account.
     */
    suspend fun switchAccount(userId: String): Result<AuthSession?> {
        authStore.setActiveSession(userId)
        return restoreSession()
    }

    /**
     * Get the active session.
     */
    fun getActiveSession(): AuthSession? {
        return authStore.activeSession()
    }

    /**
     * Get all saved sessions.
     */
    fun getSavedSessions(): List<AuthSession> {
        return authStore.state.value.sessions
    }

    /**
     * Check if a loopback login failure occurred.
     */
    private fun isLoopbackLoginFailure(error: Throwable): Boolean {
        val message = generateSequence(error) { it.cause }
            .mapNotNull { it.message }
            .joinToString(" ")
            .lowercase()
        return "oauth callback" in message ||
            "callback ports" in message ||
            "local callback" in message ||
            "localhost" in message ||
            "127.0.0.1" in message
    }
}
