package cloud.gitwide.ampverse;

import static org.junit.Assert.*;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import org.json.JSONArray;
import org.json.JSONObject;
import org.junit.Test;

public class SmartFilterContractTest {
    @Test public void everyEngineFieldHasTheSamePhoneNameKindAndLabel() throws Exception {
        try (InputStream input = getClass().getClassLoader().getResourceAsStream("smart_fields.json")) {
            assertNotNull("Shared Rust/Android contract must be packaged in test resources", input);
            ByteArrayOutputStream bytes = new ByteArrayOutputStream();
            byte[] buffer = new byte[4096];
            int count;
            while ((count = input.read(buffer)) != -1) bytes.write(buffer, 0, count);
            JSONObject contract = new JSONObject(bytes.toString(StandardCharsets.UTF_8.name()));
            assertEquals(1, contract.getInt("schema"));
            JSONArray fields = contract.getJSONArray("fields");
            assertEquals(fields.length(), PhoneSmartRules.FIELDS.length);
            assertEquals(fields.length(), PhoneSmartRules.LABELS.length);
            for (int i = 0; i < fields.length(); i++) {
                JSONObject field = fields.getJSONObject(i);
                String name = field.getString("field");
                assertEquals(name, PhoneSmartRules.FIELDS[i]);
                assertEquals(name, field.getString("label"), PhoneSmartRules.LABELS[i]);
                assertEquals(name, field.getString("kind"), PhoneSmartRules.kind(name));
            }
        }
    }
}
