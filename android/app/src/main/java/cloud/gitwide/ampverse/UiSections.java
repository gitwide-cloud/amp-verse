package cloud.gitwide.ampverse;

import android.view.View;
import android.widget.*;

/** Progressive disclosure without removing any controls or rebuilding playback. */
final class UiSections {
    static LinearLayout fold(LinearLayout parent,Appearance colors,String title,int first){
        LinearLayout body=new LinearLayout(parent.getContext());body.setOrientation(LinearLayout.VERTICAL);
        while(parent.getChildCount()>first){View child=parent.getChildAt(first);parent.removeViewAt(first);body.addView(child);}
        Button toggle=new Button(parent.getContext());toggle.setText(title+"  +");toggle.setAllCaps(false);toggle.setTextSize(14);toggle.setTextColor(colors.text);toggle.setBackground(Chrome.button(colors,false));toggle.setMinHeight((int)(48*parent.getResources().getDisplayMetrics().density));toggle.setContentDescription(title);parent.addView(toggle);parent.addView(body);android.content.SharedPreferences prefs=parent.getContext().getSharedPreferences("ui-sections",0);String key=parent.getContext().getClass().getSimpleName()+":"+title;boolean expanded=prefs.getBoolean(key,false);body.setVisibility(expanded?View.VISIBLE:View.GONE);toggle.setText(title+(expanded?"  −":"  +"));
        toggle.setOnClickListener(v->{boolean open=body.getVisibility()!=View.VISIBLE;body.setVisibility(open?View.VISIBLE:View.GONE);toggle.setText(title+(open?"  −":"  +"));prefs.edit().putBoolean(key,open).apply();});return body;
    }
}
