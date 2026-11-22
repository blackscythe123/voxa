package com.voxa.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.voxa.android.ui.icons.VoxaIcons
import com.voxa.android.ui.theme.DisplayFamily
import com.voxa.android.ui.theme.MonoFamily
import com.voxa.android.ui.theme.VoxaColors

@Composable
fun SubpageScaffold(
    title: String,
    onBack: () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(VoxaColors.Bg)
            .systemBarsPadding(),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = 8.dp, end = 22.dp, top = 10.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                modifier = Modifier
                    .size(40.dp)
                    .clip(CircleShape)
                    .clickable { onBack() },
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    imageVector = VoxaIcons.ChevronLeft,
                    contentDescription = "Back",
                    tint = VoxaColors.Ink,
                    modifier = Modifier.size(22.dp),
                )
            }
            Spacer(Modifier.width(4.dp))
            Text(
                text = title,
                fontFamily = DisplayFamily,
                fontWeight = FontWeight.SemiBold,
                fontSize = 22.sp,
                letterSpacing = (-0.5).sp,
                color = VoxaColors.Ink,
            )
        }
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState()),
            content = content,
        )
    }
}

@Composable
fun SectionHeader(title: String) {
    Text(
        text = title,
        fontFamily = DisplayFamily,
        fontWeight = FontWeight.SemiBold,
        fontSize = 13.sp,
        letterSpacing = (-0.2).sp,
        color = VoxaColors.Ink,
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 26.dp)
            .padding(top = 20.dp, bottom = 8.dp),
    )
}

@Composable
fun Panel(content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(VoxaColors.Surface)
            .border(1.dp, VoxaColors.Hair, RoundedCornerShape(12.dp)),
        content = content,
    )
}

@Composable
fun HairDivider() {
    HorizontalDivider(color = VoxaColors.HairSoft, thickness = 1.dp)
}

@Composable
fun Chevron(tint: Color = VoxaColors.MutedSoft) {
    Icon(
        imageVector = VoxaIcons.Chevron,
        contentDescription = null,
        tint = tint,
        modifier = Modifier.size(16.dp),
    )
}

@Composable
fun SettingsRow(
    icon: ImageVector?,
    name: String,
    sub: String? = null,
    destructive: Boolean = false,
    onClick: (() -> Unit)? = null,
    trailing: @Composable RowScope.() -> Unit = {},
) {
    val rowColor = if (destructive) VoxaColors.Destructive else VoxaColors.Ink
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable { onClick() } else Modifier)
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Box(
                modifier = Modifier
                    .size(30.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(if (destructive) VoxaColors.DestructiveSoft else VoxaColors.IconChip),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    imageVector = icon,
                    contentDescription = null,
                    tint = if (destructive) VoxaColors.Destructive else VoxaColors.Ink,
                    modifier = Modifier.size(18.dp),
                )
            }
            Spacer(Modifier.width(13.dp))
        }
        Column(modifier = Modifier.weight(1f)) {
            Text(name, color = rowColor, fontSize = 14.5.sp, fontWeight = FontWeight.Medium)
            if (sub != null) {
                Text(sub, color = VoxaColors.Muted, fontSize = 11.5.sp, modifier = Modifier.padding(top = 2.dp))
            }
        }
        trailing()
    }
}

@Composable
fun quietSwitchColors() = SwitchDefaults.colors(
    checkedTrackColor = VoxaColors.Primary,
    uncheckedTrackColor = Color(0xFFD6D2C7),
    checkedThumbColor = Color.White,
    uncheckedThumbColor = Color.White,
    checkedBorderColor = VoxaColors.Primary,
    uncheckedBorderColor = Color(0xFFD6D2C7),
)

@Composable
fun StatusPill(connected: Boolean) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .clip(RoundedCornerShape(99.dp))
            .background(if (connected) VoxaColors.SuccessSoft else VoxaColors.DestructiveSoft)
            .padding(horizontal = 10.dp, vertical = 5.dp),
    ) {
        Box(
            modifier = Modifier
                .size(5.dp)
                .clip(CircleShape)
                .background(if (connected) VoxaColors.Success else VoxaColors.Destructive)
        )
        Spacer(Modifier.width(6.dp))
        Text(
            text = if (connected) "Connected" else "Expired",
            fontFamily = MonoFamily,
            fontSize = 10.sp,
            color = if (connected) VoxaColors.SuccessFg else VoxaColors.Destructive,
        )
    }
}
