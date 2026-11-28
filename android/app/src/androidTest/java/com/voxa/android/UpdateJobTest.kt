package com.voxa.android

import android.app.job.JobScheduler
import android.net.NetworkCapabilities
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.voxa.android.notify.Updater
import com.voxa.android.notify.VoxaJobs
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/** The update jobs on the framework JobScheduler (7104 check, 7105 download). */
@RunWith(AndroidJUnit4::class)
class UpdateJobTest {
    private val ctx = InstrumentationRegistry.getInstrumentation().targetContext
    private val js = ctx.getSystemService(JobScheduler::class.java)
    private var auto = true

    @Before
    fun keep() {
        auto = Updater.auto(ctx)
        VoxaJobs.cancelUpdateJobs(ctx)
    }

    @After
    fun restore() {
        VoxaJobs.cancelUpdateJobs(ctx)
        Updater.setAuto(ctx, auto)
    }

    @Test
    fun the_update_check_job_is_periodic_and_persisted() {
        Updater.setAuto(ctx, true)
        val j = js.getPendingJob(VoxaJobs.UPDATE_CHECK)
        assertNotNull(j)
        assertTrue(j!!.isPeriodic)
        assertEquals(12 * 60 * 60_000L, j.intervalMillis)
        assertTrue(j.isPersisted)
        assertTrue(j.isRequireBatteryNotLow)
    }

    @Test
    fun the_update_download_needs_an_unmetered_network() {
        Updater.setAuto(ctx, true)
        // What the scheduler is given (on the emulator's Wi-Fi the job would run at once).
        val j = VoxaJobs.updateDownloadJob(ctx, 6_000_000L, anyNetwork = false)
        assertEquals(VoxaJobs.UPDATE_DOWNLOAD, j.id)
        assertFalse(j!!.isPeriodic)
        assertTrue(j.isPersisted)
        assertTrue(j.isRequireBatteryNotLow)
        @Suppress("DEPRECATION")
        assertEquals(android.app.job.JobInfo.NETWORK_TYPE_UNMETERED, j.networkType)
        assertTrue(j.requiredNetwork!!.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED))
        assertEquals(6_000_000L, j.estimatedNetworkDownloadBytes)
    }

    @Test
    fun neither_update_job_is_scheduled_with_the_setting_off() {
        Updater.setAuto(ctx, true)
        Updater.setAuto(ctx, false)
        assertNull(js.getPendingJob(VoxaJobs.UPDATE_CHECK))
        assertNull(js.getPendingJob(VoxaJobs.UPDATE_DOWNLOAD))
        VoxaJobs.scheduleUpdateCheck(ctx)
        assertNull(js.getPendingJob(VoxaJobs.UPDATE_CHECK))
    }
}
