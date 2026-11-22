package com.voxa.android.ui.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

// Phosphor-style 1.5px-stroke icons, 24x24 viewport, stroked using the host text color.
// Use Icon(imageVector = VoxaIcons.Keyboard, tint = VoxaColors.Ink) to render.

object VoxaIcons {

    val Keyboard: ImageVector = strokedIcon("Keyboard") {
        // rect outline
        moveTo(3f, 6f); lineTo(21f, 6f); arcTo(2f, 2f, 0f, false, true, 23f, 8f); lineTo(23f, 18f); arcTo(2f, 2f, 0f, false, true, 21f, 20f); lineTo(3f, 20f); arcTo(2f, 2f, 0f, false, true, 1f, 18f); lineTo(1f, 8f); arcTo(2f, 2f, 0f, false, true, 3f, 6f); close()
        // dots row 1
        moveTo(6.5f, 10.5f); lineTo(6.5f, 10.51f)
        moveTo(10.5f, 10.5f); lineTo(10.5f, 10.51f)
        moveTo(14.5f, 10.5f); lineTo(14.5f, 10.51f)
        moveTo(18.5f, 10.5f); lineTo(18.5f, 10.51f)
        // space bar
        moveTo(7f, 16f); lineTo(17f, 16f)
    }

    val Star: ImageVector = strokedIcon("Star") {
        moveTo(12f, 3f); lineTo(14.5f, 9f); lineTo(20.5f, 9.8f); lineTo(16.2f, 14f); lineTo(17.2f, 20f); lineTo(12f, 16.8f); lineTo(6.8f, 20f); lineTo(7.8f, 14f); lineTo(3.5f, 9.8f); lineTo(9.5f, 9f); close()
    }

    val Swap: ImageVector = strokedIcon("Swap") {
        moveTo(3f, 8f); lineTo(17f, 8f); lineTo(14f, 5f)
        moveTo(21f, 16f); lineTo(7f, 16f); lineTo(10f, 19f)
    }

    val Infinity: ImageVector = strokedIcon("Infinity") {
        moveTo(5f, 12f)
        curveTo(5f, 9f, 7.2f, 7f, 10f, 7f)
        curveTo(12f, 7f, 13.4f, 8.2f, 14.5f, 9.8f)
        curveTo(15.6f, 11.4f, 17f, 13f, 19f, 13f)
        curveTo(20.7f, 13f, 22f, 11.7f, 22f, 10f)
        curveTo(22f, 8.3f, 20.7f, 7f, 19f, 7f)
        curveTo(17f, 7f, 15.6f, 8.6f, 14.5f, 10.2f)
        curveTo(13.4f, 11.8f, 12f, 13f, 10f, 13f)
        curveTo(7.2f, 13f, 5f, 11f, 5f, 8f)
    }

    val Gauge: ImageVector = strokedIcon("Gauge") {
        moveTo(3f, 12f); lineTo(6f, 12f); lineTo(8f, 6f); lineTo(12f, 18f); lineTo(15f, 9f); lineTo(17f, 13f); lineTo(21f, 12f)
    }

    val Pulse: ImageVector = strokedIcon("Pulse") {
        moveTo(3f, 12f); lineTo(6f, 12f)
        moveTo(18f, 12f); lineTo(21f, 12f)
        moveTo(9f, 12f); lineTo(9.01f, 12f)
        moveTo(15f, 12f); lineTo(15.01f, 12f)
    }

    val Finger: ImageVector = strokedIcon("Finger") {
        moveTo(9f, 11f); lineTo(9f, 6.5f); arcTo(2.5f, 2.5f, 0f, false, true, 14f, 6.5f); lineTo(14f, 12f); arcTo(4f, 4f, 0f, true, true, 6f, 12f); lineTo(6f, 8.5f)
        moveTo(14f, 13f); lineTo(16f, 15f); lineTo(20f, 11f)
    }

    val Mic: ImageVector = strokedIcon("Mic") {
        moveTo(9f, 3f); lineTo(15f, 3f); arcTo(3f, 3f, 0f, false, true, 18f, 6f); lineTo(18f, 11f); arcTo(3f, 3f, 0f, false, true, 15f, 14f); lineTo(9f, 14f); arcTo(3f, 3f, 0f, false, true, 6f, 11f); lineTo(6f, 6f); arcTo(3f, 3f, 0f, false, true, 9f, 3f); close()
        moveTo(5f, 11f); arcTo(7f, 7f, 0f, false, false, 19f, 11f)
        moveTo(12f, 18f); lineTo(12f, 21f)
        moveTo(9f, 21f); lineTo(15f, 21f)
    }

    val Globe: ImageVector = strokedIcon("Globe") {
        moveTo(12f, 3f); arcTo(9f, 9f, 0f, true, true, 11.99f, 3f); close()
        moveTo(3f, 12f); lineTo(21f, 12f)
        moveTo(12f, 3f); curveTo(15.5f, 7f, 15.5f, 17f, 12f, 21f)
        moveTo(12f, 3f); curveTo(8.5f, 7f, 8.5f, 17f, 12f, 21f)
    }

    val Chip: ImageVector = strokedIcon("Chip") {
        moveTo(4f, 4f); lineTo(20f, 4f); arcTo(0f, 0f, 0f, false, true, 20f, 4f); lineTo(20f, 20f); lineTo(4f, 20f); close()
        moveTo(9f, 9f); lineTo(15f, 9f); lineTo(15f, 15f); lineTo(9f, 15f); close()
        moveTo(9f, 2f); lineTo(9f, 4f)
        moveTo(15f, 2f); lineTo(15f, 4f)
        moveTo(9f, 20f); lineTo(9f, 22f)
        moveTo(15f, 20f); lineTo(15f, 22f)
        moveTo(2f, 9f); lineTo(4f, 9f)
        moveTo(2f, 15f); lineTo(4f, 15f)
        moveTo(20f, 9f); lineTo(22f, 9f)
        moveTo(20f, 15f); lineTo(22f, 15f)
    }

    val Period: ImageVector = strokedIcon("Period") {
        moveTo(6f, 18f); arcTo(1.5f, 1.5f, 0f, true, true, 5.99f, 18f); close()
        moveTo(11f, 6f); lineTo(21f, 6f)
        moveTo(11f, 12f); lineTo(21f, 12f)
        moveTo(11f, 18f); lineTo(17f, 18f)
    }

    val FillerMinus: ImageVector = strokedIcon("FillerMinus") {
        moveTo(5f, 10f); lineTo(19f, 10f)
        moveTo(5f, 14f); lineTo(14f, 14f)
    }

    val Speaker: ImageVector = strokedIcon("Speaker") {
        moveTo(3f, 9f); lineTo(3f, 15f); lineTo(7f, 15f); lineTo(12f, 19f); lineTo(12f, 5f); lineTo(7f, 9f); close()
        moveTo(17f, 8f); arcTo(5f, 5f, 0f, false, true, 17f, 16f)
        moveTo(20f, 5f); arcTo(9f, 9f, 0f, false, true, 20f, 19f)
    }

    val CircleCheck: ImageVector = strokedIcon("CircleCheck") {
        moveTo(12f, 3f); arcTo(9f, 9f, 0f, true, true, 11.99f, 3f); close()
        moveTo(8f, 12f); lineTo(11f, 15f); lineTo(16f, 9f)
    }

    val Refresh: ImageVector = strokedIcon("Refresh") {
        moveTo(3f, 12f); arcTo(9f, 9f, 0f, false, true, 18f, 5.3f); lineTo(21f, 8f)
        moveTo(21f, 3f); lineTo(21f, 8f); lineTo(16f, 8f)
        moveTo(21f, 12f); arcTo(9f, 9f, 0f, false, true, 6f, 18.7f); lineTo(3f, 16f)
        moveTo(3f, 21f); lineTo(3f, 16f); lineTo(8f, 16f)
    }

    val SignOut: ImageVector = strokedIcon("SignOut") {
        moveTo(9f, 3f); lineTo(5f, 3f); arcTo(2f, 2f, 0f, false, false, 3f, 5f); lineTo(3f, 19f); arcTo(2f, 2f, 0f, false, false, 5f, 21f); lineTo(9f, 21f)
        moveTo(16f, 17f); lineTo(21f, 12f); lineTo(16f, 7f)
        moveTo(21f, 12f); lineTo(9f, 12f)
    }

    val Info: ImageVector = strokedIcon("Info") {
        moveTo(12f, 3f); arcTo(9f, 9f, 0f, true, true, 11.99f, 3f); close()
        moveTo(12f, 8f); lineTo(12f, 8.01f)
        moveTo(11f, 12f); lineTo(12f, 12f); lineTo(12f, 17f); lineTo(13f, 17f)
    }

    val Hash: ImageVector = strokedIcon("Hash") {
        moveTo(4f, 9f); lineTo(20f, 9f)
        moveTo(4f, 15f); lineTo(20f, 15f)
        moveTo(10f, 3f); lineTo(8f, 21f)
        moveTo(16f, 3f); lineTo(14f, 21f)
    }

    val Send: ImageVector = strokedIcon("Send") {
        moveTo(12f, 19f); lineTo(12f, 5f)
        moveTo(5f, 12f); lineTo(12f, 5f); lineTo(19f, 12f)
    }

    val Chevron: ImageVector = strokedIcon("Chevron") {
        moveTo(9f, 6f); lineTo(15f, 12f); lineTo(9f, 18f)
    }

    val ChevronLeft: ImageVector = strokedIcon("ChevronLeft") {
        moveTo(15f, 6f); lineTo(9f, 12f); lineTo(15f, 18f)
    }

    val Book: ImageVector = strokedIcon("Book") {
        moveTo(4f, 4f); lineTo(4f, 20f); lineTo(18f, 20f); arcTo(2f, 2f, 0f, false, false, 20f, 18f); lineTo(20f, 6f); arcTo(2f, 2f, 0f, false, false, 18f, 4f); close()
        moveTo(8f, 4f); lineTo(8f, 20f)
        moveTo(12f, 9f); lineTo(16f, 9f)
        moveTo(12f, 13f); lineTo(16f, 13f)
    }

    val Settings: ImageVector = strokedIcon("Settings") {
        moveTo(12f, 9f); arcTo(3f, 3f, 0f, true, true, 11.99f, 9f); close()
        moveTo(12f, 3f); lineTo(12f, 5f)
        moveTo(12f, 19f); lineTo(12f, 21f)
        moveTo(3f, 12f); lineTo(5f, 12f)
        moveTo(19f, 12f); lineTo(21f, 12f)
        moveTo(5.6f, 5.6f); lineTo(7f, 7f)
        moveTo(17f, 17f); lineTo(18.4f, 18.4f)
        moveTo(5.6f, 18.4f); lineTo(7f, 17f)
        moveTo(17f, 7f); lineTo(18.4f, 5.6f)
    }

    val User: ImageVector = strokedIcon("User") {
        moveTo(12f, 4f); arcTo(4f, 4f, 0f, true, true, 11.99f, 4f); close()
        moveTo(4f, 21f); arcTo(8f, 8f, 0f, false, true, 20f, 21f)
    }

    val Question: ImageVector = strokedIcon("Question") {
        moveTo(12f, 3f); arcTo(9f, 9f, 0f, true, true, 11.99f, 3f); close()
        moveTo(9f, 9f); arcTo(3f, 3f, 0f, false, true, 12f, 6f); arcTo(3f, 3f, 0f, false, true, 14.5f, 10.5f); lineTo(12f, 13f); lineTo(12f, 14f)
        moveTo(12f, 17.5f); lineTo(12f, 17.51f)
    }

    val Backspace: ImageVector = strokedIcon("Backspace") {
        moveTo(21f, 5f); lineTo(8f, 5f); lineTo(3f, 12f); lineTo(8f, 19f); lineTo(21f, 19f); arcTo(0f, 0f, 0f, false, false, 21f, 19f); lineTo(21f, 5f); close()
        moveTo(11f, 9f); lineTo(17f, 15f)
        moveTo(17f, 9f); lineTo(11f, 15f)
    }

    val Shift: ImageVector = strokedIcon("Shift") {
        moveTo(12f, 4f); lineTo(4f, 12f); lineTo(8f, 12f); lineTo(8f, 20f); lineTo(16f, 20f); lineTo(16f, 12f); lineTo(20f, 12f); close()
    }

    val ShiftFilled: ImageVector = strokedIcon("ShiftFilled") {
        moveTo(12f, 4f); lineTo(4f, 12f); lineTo(8f, 12f); lineTo(8f, 20f); lineTo(16f, 20f); lineTo(16f, 12f); lineTo(20f, 12f); close()
        moveTo(8f, 18f); lineTo(16f, 18f)
    }

    val Stop: ImageVector = strokedIcon("Stop") {
        moveTo(7f, 7f); lineTo(17f, 7f); lineTo(17f, 17f); lineTo(7f, 17f); close()
    }

    val Return: ImageVector = strokedIcon("Return") {
        moveTo(20f, 6f); lineTo(20f, 12f); arcTo(2f, 2f, 0f, false, true, 18f, 14f); lineTo(5f, 14f)
        moveTo(9f, 10f); lineTo(5f, 14f); lineTo(9f, 18f)
    }

    val Smiley: ImageVector = strokedIcon("Smiley") {
        moveTo(12f, 3f); arcTo(9f, 9f, 0f, true, true, 11.99f, 3f); close()
        moveTo(9f, 10f); lineTo(9f, 10.01f)
        moveTo(15f, 10f); lineTo(15f, 10.01f)
        moveTo(8.5f, 14f); curveTo(9.5f, 16f, 14.5f, 16f, 15.5f, 14f)
    }
}

private inline fun strokedIcon(name: String, crossinline pathSpec: androidx.compose.ui.graphics.vector.PathBuilder.() -> Unit): ImageVector =
    ImageVector.Builder(
        name = name,
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f,
    ).apply {
        path(
            fill = null,
            stroke = SolidColor(Color.Black),
            strokeLineWidth = 1.5f,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round,
            pathFillType = PathFillType.NonZero,
            pathBuilder = { pathSpec() },
        )
    }.build()
